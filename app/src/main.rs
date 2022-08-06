#![feature(alloc_error_handler)]
#![feature(trait_alias)]
#![feature(type_alias_impl_trait)]
#![no_main]
#![no_std]

extern crate alloc;

mod alarms;
mod allocator;
mod debounce;
mod encoder;
mod gate_cv;
mod midi_in;
mod screen;
mod mpmc;
mod switches;
mod task_queues;

use alloc::{borrow::ToOwned, format, string::String};
use allocator::CortexMHeap;
use critical_section::{Mutex, with};
use embassy_util::blocking_mutex::raw::CriticalSectionRawMutex;
use core::{alloc::Layout, fmt::Debug};
use embassy_executor::executor::{raw::TaskPool, Executor};
use futures::Future;
use gate_cv::GateCVProxy;

use defmt::panic;
use embedded_sdmmc::{sdmmc::BlockSpi, TimeSource, Timestamp};
use shared_bus::{BusManagerSimple, NullMutex, SpiProxy};

use core::{cell::RefCell, convert::Into, ops::DerefMut};

use cortex_m::{
    singleton,
};
use cortex_m_rt::entry;
use defmt::*;
use defmt_rtt as _;
use panic_probe as _;

use embedded_graphics::{pixelcolor::Rgb565, prelude::RgbColor, prelude::*};
use embedded_hal::blocking::spi::{Transfer, Write};
use embedded_hal::spi::{MODE_0, MODE_3};
use embedded_time::{fixed_point::FixedPoint, rate::Extensions};
use rp2040_hal as hal;

use hal::{
    clocks::{init_clocks_and_plls, Clock},
    gpio::{
        pin::{bank0::Gpio8, Pin},
        Output, PushPull,
    },
    multicore::{Multicore, Stack},
    pac::{self},
    pac::{interrupt, Interrupt, Peripherals, NVIC},
    sio::Sio,
    spi::Enabled,
    watchdog::Watchdog,
    Spi, Timer,
};

use logic::{
    programs::{self, Program, ProgramError},
    stdlib::{StdlibError, Task, TaskReturn},
    LogLevel,
};
use screen::{Framebuffer, ScreenDriverWithPins};
use task_queues::EmbeddedTaskInterface;

use crate::task_queues::task_manager;

#[link_section = ".boot2"]
#[no_mangle]
#[used]
pub static BOOT2_FIRMWARE: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;

#[global_allocator]
static ALLOCATOR: CortexMHeap = CortexMHeap::empty();

#[alloc_error_handler]
fn oom(_: Layout) -> ! {
    panic!("OOM");
}

pub static TIMER: Mutex<RefCell<Option<Timer>>> = Mutex::new(RefCell::new(None));

const PERIPHERAL_FREQ: u32 = 125_000_000u32;
const EXTERNAL_XTAL_FREQ: u32 = 12_000_000u32;

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
    mut delay: cortex_m::delay::Delay,
    mut task_iface: EmbeddedTaskInterface<'t>,
    mut output: GateCVProxy,
) -> Result<(), StdlibError>
where
    SpiProxy<'t, NullMutex<Spi<Enabled, pac::SPI0, 8>>>: Transfer<u8>,
    <SpiProxy<'t, NullMutex<Spi<Enabled, pac::SPI0, 8>>> as Transfer<u8>>::Error: Debug,
{
    info!("Starting main loop");

    let buffer_addr = unsafe { scr.buffer_addr() };

    program.setup();

    loop {
        with(|cs| {
            if let Some(midi_in) = midi_in::MIDI_IN.borrow(cs).borrow_mut().deref_mut() {
                for msg in midi_in.iter_messages() {
                    program.process_midi(&msg)
                }
            }
        });
        with(|cs| -> Result<(), ProgramError> {
            if let Some(encoder) = encoder::ROTARY_ENCODER.borrow(cs).borrow_mut().deref_mut() {
                for msg in encoder.iter_messages() {
                    program.process_ui_input(&msg)?;
                    // let mut s = String::<32>::new();
                    // uwrite!(s, "{:#?}", msg);
                    // info!("{}", s);
                }
            }
            Ok(())
        })
        .map_err(|ProgramError::Stdlib(e)| e)?;
        let prog_time = with(|cs| -> Result<u32, ProgramError> {
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
        })
        .map_err(|ProgramError::Stdlib(e)| e)?;

        with(|_| {
            program.run(prog_time, &mut task_iface);
            program.update_output(&mut output).unwrap();
        });

        scr.clear(Rgb565::BLACK).unwrap();
        // scr.clear(Rgb565::new(((prog_time * 23) % 255) as u8, (prog_time % 255) as u8, ((prog_time * 31) % 255) as u8)).unwrap();

        program.render_screen(scr);

        let mut p = unsafe { pac::Peripherals::steal() };

        screen::refresh(
            &mut p.DMA,
            p.SPI0,
            buffer_addr,
            &mut screen_driver,
            &mut delay,
        )
        .await;
    }
}

fn run_executor<F: Future + 'static>(id: u8, f: F) -> ! {
    let mut task_pool = TaskPool::<F, 1>::new();
    let task_pool: &mut TaskPool<F, 1> = unsafe { core::mem::transmute(&mut task_pool) };

    let mut executor: Executor = Executor::new();
    let executor: &mut Executor = unsafe { core::mem::transmute(&mut executor) };

    info!("Starting Core {} executor...", id);

    executor.run(|spawner| {
        let token = task_pool.spawn(move || f);
        spawner.must_spawn(token);
    });
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
    let external_xtal_freq_hz = EXTERNAL_XTAL_FREQ;
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
        PERIPHERAL_FREQ.Hz(),
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

    let prog_queue = singleton!(: mpmc::Channel<CriticalSectionRawMutex, TaskReturn, 16> = mpmc::Channel::new()).unwrap();
    let tm_queue = singleton!(: mpmc::Channel<CriticalSectionRawMutex, Task, 16> = mpmc::Channel::new()).unwrap();
    let (tm_send, prog_recv) = (prog_queue.sender(), prog_queue.receiver());
    let (prog_send, tm_recv) = (tm_queue.sender(), tm_queue.receiver());
    let task_iface = EmbeddedTaskInterface::new(prog_recv, prog_send);

    static mut CORE1_STACK: Stack<10240> = Stack::new();
    let _core1 = core1.spawn(unsafe { &mut CORE1_STACK.mem }, move || {
        let spi_bus = BusManagerSimple::new(Spi::<_, _, 8>::new(pac.SPI1).init(
            &mut pac.RESETS,
            PERIPHERAL_FREQ.Hz(),
            400_000u32.Hz(),
            &MODE_0,
        ));

        let pins = (
            pins.gpio10.into_mode::<hal::gpio::FunctionSpi>(),
            pins.gpio12.into_mode::<hal::gpio::FunctionSpi>(),
            pins.gpio11.into_mode::<hal::gpio::FunctionSpi>(),
            pins.gpio9.into_push_pull_output(),
            pins.gpio8.into_push_pull_output(),
            // gates
            pins.gpio4.into_push_pull_output(),
            pins.gpio5.into_push_pull_output(),
        );

        info!("Core 1 reporting");

        let fut = task_manager(
            tm_recv,
            tm_send,
            spi_bus,
            pins,
        );

        run_executor(1, fut);
    });

    init_interrupts(&mut timer);

    with(|cs| {
        let mut timer_singleton = TIMER.borrow(cs).borrow_mut();
        timer_singleton.replace(timer);
    });

    // enable IRQs
    unsafe {
        NVIC::unmask(Interrupt::IO_IRQ_BANK0);
        NVIC::unmask(Interrupt::SPI0_IRQ);
        NVIC::unmask(Interrupt::UART0_IRQ);
        NVIC::unmask(Interrupt::DMA_IRQ_0);
        NVIC::unmask(Interrupt::TIMER_IRQ_0);
    }

    let output = GateCVProxy::new();

    run_executor(
        0,
        main_loop(program, scr, screen_driver, delay, task_iface, output),
    )
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
    with(|cs| {
        let mut pac = unsafe { Peripherals::steal() };
        encoder::handle_irq(cs, &mut pac);
        switches::handle_irq(cs, &mut pac);
    });
}

#[interrupt]
fn TIMER_IRQ_0() {
    with(|cs| {
        let mut pac = unsafe { Peripherals::steal() };
        alarms::handle_irq(cs, &mut pac);
    });
}

#[interrupt]
fn SPI0_IRQ() {
    with(|cs| {
        let mut pac = unsafe { Peripherals::steal() };
        screen::handle_spi_irq(cs, &mut pac);
    });
}

#[interrupt]
fn DMA_IRQ_0() {
    with(|cs| {
        let mut pac = unsafe { Peripherals::steal() };
        screen::handle_dma_irq(cs, &mut pac);
    });
}

#[interrupt]
fn UART0_IRQ() {
    with(|cs| {
        let mut pac = unsafe { Peripherals::steal() };
        midi_in::handle_irq(cs, &mut pac);
    });
}
