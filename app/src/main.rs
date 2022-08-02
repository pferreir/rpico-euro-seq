#![feature(alloc_error_handler)]
#![feature(trait_alias)]
#![no_main]
#![no_std]

extern crate alloc;

use alloc::{format, string::String};
use allocator::CortexMHeap;
use core::{alloc::Layout, borrow::BorrowMut, fmt::Debug, task::Poll};
use gate_cv::{GateCVOut, GateCVOutWithPins, GateCVProxy};
use heapless::spsc::{self, Queue};

#[global_allocator]
static ALLOCATOR: CortexMHeap = CortexMHeap::empty();

#[alloc_error_handler]
fn oom(_: Layout) -> ! {
    panic!("OOM");
}

mod alarms;
mod allocator;
mod debounce;
mod encoder;
mod gate_cv;
mod midi_in;
mod screen;
mod switches;

use defmt::panic;
use embedded_sdmmc::{
    sdmmc::{BlockSpi, SdMmcSpi},
    TimeSource, Timestamp,
};
use shared_bus::{BusManagerSimple, NullMutex, SpiProxy};

use core::{cell::RefCell, convert::Into, ops::DerefMut};

use cassette::{pin_mut, Cassette};
use cortex_m::{
    interrupt::{free, Mutex},
    singleton,
};
use cortex_m_rt::entry;
use defmt::*;
use defmt_rtt as _;
use panic_probe as _;

use embedded_graphics::{pixelcolor::Rgb565, prelude::RgbColor, prelude::*};
use embedded_hal::spi::{MODE_0, MODE_3};
use embedded_hal::{
    blocking::spi::{Transfer, Write},
    digital::v2::OutputPin,
};
use embedded_time::{fixed_point::FixedPoint, rate::Extensions};
use futures::{future::join, Sink, Stream};
use rp2040_hal as hal;

use hal::{
    clocks::{init_clocks_and_plls, Clock},
    gpio::{
        bank0::{Gpio10, Gpio11, Gpio12, Gpio4, Gpio5, Gpio9},
        pin::{bank0::Gpio8, Pin},
        FunctionSpi, Output, PushPull, PushPullOutput,
    },
    multicore::{Multicore, Stack},
    pac::{self},
    pac::{interrupt, Interrupt, Peripherals, NVIC},
    sio::Sio,
    spi::{Disabled, Enabled, SpiDevice},
    watchdog::Watchdog,
    Spi, Timer,
};

use logic::{
    programs::{self, Program, ProgramError},
    stdlib::{
        FileSystem, StdlibError, Task, TaskId, TaskInterface, TaskManager, TaskReturn, TaskType,
    },
    LogLevel,
};
use screen::{Framebuffer, ScreenDriverWithPins};

#[link_section = ".boot2"]
#[no_mangle]
#[used]
pub static BOOT2_FIRMWARE: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;

pub static TIMER: Mutex<RefCell<Option<Timer>>> = Mutex::new(RefCell::new(None));
pub static CHANNELS: Mutex<RefCell<Option<(TaskChannelConsumer, TaskChannelProducer)>>> =
    Mutex::new(RefCell::new(None));

pub struct TaskChannelConsumer<'t>(spsc::Consumer<'t, Task, 128>);
pub struct TaskChannelProducer<'t>(spsc::Producer<'t, TaskReturn, 128>);

impl<'t> Stream for TaskChannelConsumer<'t> {
    type Item = Task;

    fn poll_next(
        mut self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Option<Self::Item>> {
        match self.0.dequeue() {
            Some(t) => Poll::Ready(Some(t)),
            None => Poll::Ready(None),
        }
    }
}

pub enum TaskChannelError {
    QueueFull,
}

impl<'t> Sink<TaskReturn> for TaskChannelProducer<'t> {
    type Error = TaskChannelError;

    fn poll_ready(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        if self.0.ready() {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }

    fn start_send(
        mut self: core::pin::Pin<&mut Self>,
        item: TaskReturn,
    ) -> Result<(), Self::Error> {
        self.0
            .enqueue(item)
            .map_err(|_| TaskChannelError::QueueFull)
    }

    fn poll_flush(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

#[inline(never)]
#[no_mangle]
unsafe fn _log(text: *const str, level: LogLevel) {
    let text = text.as_ref().unwrap();
    match level {
        LogLevel::Debug => debug!("[APP] {}", text),
        LogLevel::Info => info!("[APP] {}", text),
        LogLevel::Warning => warn!("[APP] {}", text),
        LogLevel::Error => error!("[APP] {}", text),
    }
}

struct DummyTime;

impl TimeSource for DummyTime {
    fn get_timestamp(&self) -> Timestamp {
        Timestamp {
            year_since_1970: 52,
            zero_indexed_month: 6,
            zero_indexed_day: 21,
            hours: 10,
            minutes: 0,
            seconds: 0,
        }
    }
}

struct EmbeddedTaskInterface<'t> {
    consumer: spsc::Consumer<'t, TaskReturn, 128>,
    producer: spsc::Producer<'t, Task, 128>,
    id_count: u32,
}

#[derive(Debug)]
enum TaskInterfaceError {
    QueueFull,
}

impl<'t> EmbeddedTaskInterface<'t> {
    fn new(
        consumer: spsc::Consumer<'t, TaskReturn, 128>,
        producer: spsc::Producer<'t, Task, 128>,
    ) -> Self {
        Self {
            consumer,
            producer,
            id_count: 0,
        }
    }
}

impl<'t> TaskInterface for EmbeddedTaskInterface<'t> {
    type Error = TaskInterfaceError;

    fn submit(&mut self, task_type: TaskType) -> Result<TaskId, Self::Error> {
        let id = self.id_count;
        self.id_count += 1;
        self.producer
            .enqueue(Task(id, task_type))
            .map_err(|_| TaskInterfaceError::QueueFull)
            .map(|_| id)
    }

    fn pop(&mut self) -> Result<Option<TaskReturn>, Self::Error> {
        Ok(self.consumer.dequeue())
    }
}

type BlockDeviceType<SPI> = BlockSpi<SPI, Pin<Gpio8, Output<PushPull>>>;

trait ProgramType<'t, SPI: Transfer<u8>> =
    Program<'t, BlockDeviceType<SPI>, Framebuffer, DummyTime, EmbeddedTaskInterface<'t>>
    where
        <SPI as Transfer<u8>>::Error: Debug,
        SPI: 't;

async fn main_loop<'t>(
    mut program: impl ProgramType<'t, SpiProxy<'t, NullMutex<Spi<Enabled, pac::SPI0, 8>>>> + 't,
    scr: &mut Framebuffer,
    mut screen_driver: &mut ScreenDriverWithPins,
    delay: &mut cortex_m::delay::Delay,
    mut task_iface: &mut EmbeddedTaskInterface<'t>,
    output: &mut GateCVProxy,
) -> Result<(), StdlibError<BlockDeviceType<SpiProxy<'t, NullMutex<Spi<Enabled, pac::SPI0, 8>>>>>>
where
    SpiProxy<'t, NullMutex<Spi<Enabled, pac::SPI0, 8>>>: Transfer<u8>,
    <SpiProxy<'t, NullMutex<Spi<Enabled, pac::SPI0, 8>>> as Transfer<u8>>::Error: Debug,
{
    let buffer_addr = unsafe { scr.buffer_addr() };

    program.setup();

    loop {
        free(|cs| {
            if let Some(midi_in) = midi_in::MIDI_IN.borrow(cs).borrow_mut().deref_mut() {
                for msg in midi_in.iter_messages() {
                    program.process_midi(&msg)
                }
            }
        });
        free(
            |cs| -> Result<
                (),
                ProgramError<BlockDeviceType<SpiProxy<'t, NullMutex<Spi<Enabled, pac::SPI0, 8>>>>>,
            > {
                if let Some(encoder) = encoder::ROTARY_ENCODER.borrow(cs).borrow_mut().deref_mut() {
                    for msg in encoder.iter_messages() {
                        program.process_ui_input(&msg)?;
                        // let mut s = String::<32>::new();
                        // uwrite!(s, "{:#?}", msg);
                        // info!("{}", s);
                    }
                }
                Ok(())
            },
        )
        .map_err(|ProgramError::Stdlib(e)| e)?;
        let prog_time = free(
            |cs| -> Result<
                u32,
                ProgramError<BlockDeviceType<SpiProxy<'t, NullMutex<Spi<Enabled, pac::SPI0, 8>>>>>,
            > {
                if let Some(switches) = switches::SWITCHES.borrow(cs).borrow_mut().deref_mut() {
                    for msg in switches.iter_messages() {
                        program.process_ui_input(&msg)?;
                        // let mut s = String::<32>::new();
                        // uwrite!(s, "{:#?}", msg);
                        // info!("{}", s);
                    }
                }

                if let Some(timer) = TIMER.borrow(cs).borrow().as_ref() {
                    Ok(((timer.get_counter() / 1000) & 0xffffffff) as u32)
                } else {
                    panic!("Can't get TIMER!")
                }
            },
        )
        .map_err(|ProgramError::Stdlib(e)| e)?;

        free(|_| {
            program.run(prog_time, &mut task_iface);
            program.update_output(&mut *output).unwrap();
        });

        scr.clear(Rgb565::BLACK).unwrap();
        // scr.clear(Rgb565::new(((prog_time * 23) % 255) as u8, (prog_time % 255) as u8, ((prog_time * 31) % 255) as u8)).unwrap();

        program.render_screen(scr);

        let mut p = unsafe { pac::Peripherals::steal() };

        screen::refresh(&mut p.DMA, p.SPI0, buffer_addr, &mut screen_driver, delay).await;
    }
}

async fn update_output<SPI: Transfer<u8> + Write<u8>>(mut output: GateCVOutWithPins<SPI>)
where
    <SPI as Transfer<u8>>::Error: Debug,
    <SPI as Write<u8>>::Error: Debug,
{
    loop {
        output.update();
    }
}

async fn task_manager<SPI: Transfer<u8> + Write<u8>, CS: OutputPin>(
    spi: SdMmcSpi<SPI, CS>,
    output: GateCVOutWithPins<SPI>,
) -> Result<(), StdlibError<BlockSpi<SPI, CS>>>
where
    <SPI as Transfer<u8>>::Error: Debug,
    <SPI as Write<u8>>::Error: Debug,
{
    let bspi = spi.acquire().await.map_err(StdlibError::SPI)?;
    let fs = FileSystem::new(bspi, DummyTime).await?;
    let mut task_manager = TaskManager::new(fs);

    let (mut rx, mut tx) = free(|cs| CHANNELS.borrow(cs).borrow_mut().take().unwrap());

    join(
        task_manager.run_tasks(&mut rx, &mut tx),
        update_output(output),
    )
    .await;

    Ok(())
}

fn core1_task<'t, D: SpiDevice>(
    _sys_freq: u32,
    spi: SdMmcSpi<SpiProxy<'t, NullMutex<Spi<Enabled, D, 8>>>,Pin<Gpio8, PushPullOutput>>,
    output: GateCVOut<SpiProxy<'t, NullMutex<Spi<Enabled, D, 8>>>, Gpio10, Gpio11, Gpio9, Gpio4, Gpio5>,
) -> ! {
    let fut = task_manager(spi, output);
    pin_mut!(fut);

    let mut cm = Cassette::new(fut);

    info!("Starting task future");

    loop {
        match cm.poll_on() {
            Some(o) => match o {
                Ok(()) => {
                    error!("This shouldn't happen!");
                }
                Err(e) => {
                    let s = format!("The error: {:?}", e);
                    error!("{}", s);
                }
            },
            None => {}
        }
    }
}

#[entry]
fn main() -> ! {
    {
        use core::mem::MaybeUninit;
        const HEAP_SIZE: usize = 32 * 1024;
        static mut HEAP: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
        unsafe { ALLOCATOR.init(HEAP.as_ptr() as usize, HEAP_SIZE) }
    }

    info!("Program start");
    let mut pac = pac::Peripherals::take().unwrap();
    let core = pac::CorePeripherals::take().unwrap();
    let mut watchdog = Watchdog::new(pac.WATCHDOG);

    // set timer to zero
    pac.TIMER.timehw.write(|w| unsafe { w.bits(0) });
    pac.TIMER.timelw.write(|w| unsafe { w.bits(0) });
    let mut timer = Timer::new(pac.TIMER, &mut pac.RESETS);

    // External high-speed crystal on the pico board is 12Mhz
    let external_xtal_freq_hz = 12_000_000u32;
    let clocks = init_clocks_and_plls(
        external_xtal_freq_hz,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    let sys_freq = clocks.system_clock.freq().integer();

    // bring up DMA
    pac.RESETS.reset.modify(|_, w| w.dma().clear_bit());
    while pac.RESETS.reset_done.read().dma().bit_is_clear() {}

    let mut delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().integer());
    let mut sio = Sio::new(pac.SIO);

    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    let mut mc = Multicore::new(&mut pac.PSM, &mut pac.PPB, &mut sio.fifo);
    let cores = mc.cores();
    let core1 = &mut cores[1];

    let spi = Spi::<_, _, 8>::new(pac.SPI0).init(
        &mut pac.RESETS,
        125_000_000u32.Hz(),
        32_000_000u32.Hz(),
        &MODE_3,
    );

    let screen_pins = (
        pins.gpio18.into_mode::<hal::gpio::FunctionSpi>(),
        pins.gpio19.into_mode::<hal::gpio::FunctionSpi>(),
        pins.gpio14.into_push_pull_output(),
        pins.gpio13.into_push_pull_output(),
        pins.gpio15.into_push_pull_output(),
    );

    let (scr, screen_driver) =
        singleton!(: (Framebuffer, ScreenDriverWithPins) = screen::init_screen(
            spi,
            &mut delay,
            screen_pins
        ))
        .unwrap();

    midi_in::init_midi_in(
        &mut pac.RESETS,
        pac.UART0,
        pins.gpio1.into_mode::<hal::gpio::FunctionUart>(),
        (&clocks.peripheral_clock).into(),
    );

    let program = programs::SequencerProgram::new();

    encoder::init_encoder(
        pins.gpio21.into_floating_input(),
        pins.gpio22.into_floating_input(),
        pins.gpio0.into_floating_input(),
    );

    switches::init_switches(
        pins.gpio2.into_pull_up_input(),
        pins.gpio3.into_pull_up_input(),
    );

    init_interrupts(&mut timer);

    free(|cs| {
        let mut timer_singleton = TIMER.borrow(cs).borrow_mut();
        timer_singleton.replace(timer);
    });

    static mut CORE1_STACK: Stack<4096> = Stack::new();
    let _core1 = core1.spawn(unsafe { &mut CORE1_STACK.mem }, move || {
        let (clk, miso, mosi, cs_out, cs_mmc, gate1, gate2) = (
            pins.gpio10.into_mode::<hal::gpio::FunctionSpi>(),
            pins.gpio12.into_mode::<hal::gpio::FunctionSpi>(),
            pins.gpio11.into_mode::<hal::gpio::FunctionSpi>(),
            pins.gpio9.into_push_pull_output(),
            pins.gpio8.into_push_pull_output(),
            // gates
            pins.gpio4.into_push_pull_output(),
            pins.gpio5.into_push_pull_output(),
        );

        let spi_bus = BusManagerSimple::new(Spi::<_, _, 8>::new(pac.SPI1).init(
            &mut pac.RESETS,
            125_000_000u32.Hz(),
            400_000u32.Hz(),
            &MODE_0,
        ));
        let spi = SdMmcSpi::new(spi_bus.acquire_spi(), cs_mmc);

        let output = gate_cv::GateCVOut::new(
            &mut pac.RESETS,
            // DAC
            spi_bus.acquire_spi(),
            clk,
            mosi,
            cs_out,
            // gates
            gate1,
            gate2,
        );

        info!("Core 1 reporting");

        core1_task(
            sys_freq,
            spi,
            output
        )
    });

    unsafe {
        // enable edges in GPIO21 and GPIO22
        NVIC::unmask(Interrupt::IO_IRQ_BANK0);
        NVIC::unmask(Interrupt::SPI0_IRQ);
        NVIC::unmask(Interrupt::UART0_IRQ);
        NVIC::unmask(Interrupt::DMA_IRQ_0);
        NVIC::unmask(Interrupt::TIMER_IRQ_0);
    }

    let prog_queue = singleton!(: spsc::Queue<TaskReturn, 128> = spsc::Queue::new()).unwrap();
    let tm_queue = singleton!(: spsc::Queue<Task, 128> = spsc::Queue::new()).unwrap();
    let (tm_send, prog_recv) = prog_queue.split();
    let (prog_send, tm_recv) = tm_queue.split();

    free(|cs| {
        CHANNELS
            .borrow(cs)
            .borrow_mut()
            .replace((TaskChannelConsumer(tm_recv), TaskChannelProducer(tm_send)));
    });

    let mut task_iface = EmbeddedTaskInterface::new(prog_recv, prog_send);

    let mut output = GateCVProxy::new();
    let main_future = main_loop(
        program,
        scr,
        screen_driver,
        &mut delay,
        &mut task_iface,
        &mut output,
    );

    pin_mut!(main_future);
    let mut cm = Cassette::new(main_future);

    loop {
        match cm.poll_on() {
            Some(o) => match o {
                Ok(()) => {
                    error!("This shouldn't happen!");
                }
                Err(e) => {
                    let s = format!("The error: {:?}", e);
                    error!("{}", s);
                }
            },
            None => {}
        }
    }
}

fn init_interrupts(timer: &mut Timer) {
    let mut pac = unsafe { Peripherals::steal() };
    alarms::init_interrupts(&mut pac, timer);
    encoder::init_interrupts(&mut pac);
    screen::init_interrupts(&mut pac);
    switches::init_interrupts(&mut pac);
    midi_in::init_interrupts(&mut pac);
}

#[interrupt]
fn IO_IRQ_BANK0() {
    free(|cs| {
        let mut pac = unsafe { Peripherals::steal() };
        encoder::handle_irq(cs, &mut pac);
        switches::handle_irq(cs, &mut pac);
    });
}

#[interrupt]
fn TIMER_IRQ_0() {
    free(|cs| {
        let mut pac = unsafe { Peripherals::steal() };
        alarms::handle_irq(cs, &mut pac);
    });
}

#[interrupt]
fn SPI0_IRQ() {
    free(|cs| {
        let mut pac = unsafe { Peripherals::steal() };
        screen::handle_spi_irq(cs, &mut pac);
    });
}

#[interrupt]
fn DMA_IRQ_0() {
    free(|cs| {
        let mut pac = unsafe { Peripherals::steal() };
        screen::handle_dma_irq(cs, &mut pac);
    });
}

#[interrupt]
fn UART0_IRQ() {
    free(|cs| {
        let mut pac = unsafe { Peripherals::steal() };
        midi_in::handle_irq(cs, &mut pac);
    });
}
