#![feature(alloc_error_handler)]
#![feature(trait_alias)]
#![no_main]
#![no_std]

extern crate alloc;

use alloc::format;
use alloc_cortex_m::CortexMHeap;
use core::{alloc::Layout, fmt::Debug};

#[global_allocator]
static ALLOCATOR: CortexMHeap = CortexMHeap::empty();

#[alloc_error_handler]
fn oom(_: Layout) -> ! {
    loop {}
}

mod alarms;
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
use shared_bus::BusManagerSimple;
// use heapless::String;
// use ufmt::uwrite;

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
    pac::{self},
    pac::{interrupt, Interrupt, Peripherals, NVIC},
    sio::Sio,
    spi::Disabled,
    watchdog::Watchdog,
    Spi, Timer,
};

use logic::{
    programs::{self, Program, ProgramError},
    stdlib::{FileSystem, TaskManager, StdlibError},
    LogLevel,
};
use screen::{Framebuffer, ScreenDriverWithPins};

#[link_section = ".boot2"]
#[no_mangle]
#[used]
pub static BOOT2_FIRMWARE: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;

pub static TIMER: Mutex<RefCell<Option<Timer>>> = Mutex::new(RefCell::new(None));

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

type SpiType<SPI, PIN> = BlockSpi<SPI, PIN>;
type BlockDeviceType<SPI> = SpiType<SPI, Pin<Gpio8, Output<PushPull>>>;

trait ProgramType<'t, SPI: Transfer<u8>> =
    Program<'t, BlockDeviceType<SPI>, Framebuffer, DummyTime>
    where
        <SPI as Transfer<u8>>::Error: Debug,
        SPI: 't;

//#[derive(Debug)]
enum Error<SPI: Transfer<u8>> where
    <SPI as Transfer<u8>>::Error: Debug
{
    Spi(embedded_sdmmc::sdmmc::Error),
    Stdlib(StdlibError<BlockDeviceType<SPI>>)
}

impl<SPI: Transfer<u8>> Debug for Error<SPI> where
    <SPI as Transfer<u8>>::Error: Debug
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Spi(arg0) => f.debug_tuple("Spi").field(arg0).finish(),
            Self::Stdlib(arg0) => f.debug_tuple("Stdlib").field(arg0).finish(),
        }
    }
}

async fn main_loop<'t, SPI: Write<u8> + Transfer<u8>>(
    mut program: impl ProgramType<'t, SPI> + 't,
    spi: SPI,
    cs: Pin<Gpio8, Output<PushPull>>,
    scr: &mut Framebuffer,
    mut screen_driver: &mut ScreenDriverWithPins,
    output: &mut gate_cv::GateCVOutWithPins<SPI>,
    delay: &mut cortex_m::delay::Delay,
) -> Result<(), Error<SPI>>
where
    <SPI as Write<u8>>::Error: Debug,
{
    let buffer_addr = unsafe { scr.buffer_addr() };

    // let spi = SdMmcSpi::new(spi, cs);
    // let bspi = spi.acquire().await.map_err(Error::Spi)?;
    // let fs = FileSystem::new(bspi, DummyTime)
    //     .await
    //     .map_err(|e| Error::Stdlib(e))?;
    let mut task_manager = TaskManager::new();

    program.setup();

    loop {
        free(|cs| {
            if let Some(midi_in) = midi_in::MIDI_IN.borrow(cs).borrow_mut().deref_mut() {
                for msg in midi_in.iter_messages() {
                    program.process_midi(&msg)
                }
            }
        });
        free(|cs| -> Result<(), ProgramError<BlockDeviceType<SPI>>> {
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
        .map_err(|ProgramError::Stdlib(e)| Error::Stdlib(e))?;
        let prog_time = free(|cs| -> Result<u32, ProgramError<BlockDeviceType<SPI>>> {
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
        .map_err(|ProgramError::Stdlib(e)| Error::Stdlib(e))?;

        program.run(prog_time, &mut task_manager);

        task_manager.run_tasks(&mut program).await;

        scr.clear(Rgb565::BLACK).unwrap();
        // scr.clear(Rgb565::new(((prog_time * 23) % 255) as u8, (prog_time % 255) as u8, ((prog_time * 31) % 255) as u8)).unwrap();

        program.render_screen(scr);
        program.update_output(output);

        let mut p = unsafe { pac::Peripherals::steal() };

        screen::refresh(&mut p.DMA, p.SPI0, buffer_addr, &mut screen_driver, delay).await;
    }
}

#[entry]
fn main() -> ! {
    {
        use core::mem::MaybeUninit;
        const HEAP_SIZE: usize = 1024;
        static mut HEAP: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
        unsafe { ALLOCATOR.init(HEAP.as_ptr() as usize, HEAP_SIZE) }
    }

    info!("Program start");
    let mut pac = pac::Peripherals::take().unwrap();
    let core = pac::CorePeripherals::take().unwrap();
    let mut watchdog = Watchdog::new(pac.WATCHDOG);
    let sio = Sio::new(pac.SIO);

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

    // bring up DMA
    pac.RESETS.reset.modify(|_, w| w.dma().clear_bit());
    while pac.RESETS.reset_done.read().dma().bit_is_clear() {}

    let mut delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().integer());

    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

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
        clocks.peripheral_clock.into(),
    );

    let spi: Spi<Disabled, _, 8> = Spi::new(pac.SPI1);
    let spi_bus = BusManagerSimple::new(spi.init(
        &mut pac.RESETS,
        125_000_000u32.Hz(),
        1_000_000u32.Hz(),
        &MODE_0,
    ));

    let mut output = gate_cv::GateCVOut::new(
        &mut pac.RESETS,
        // DAC
        spi_bus.acquire_spi(),
        pins.gpio10.into_mode::<hal::gpio::FunctionSpi>(),
        pins.gpio11.into_mode::<hal::gpio::FunctionSpi>(),
        pins.gpio9.into_push_pull_output(),
        // gates
        pins.gpio4.into_push_pull_output(),
        pins.gpio5.into_push_pull_output(),
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

    unsafe {
        // enable edges in GPIO21 and GPIO22
        NVIC::unmask(Interrupt::IO_IRQ_BANK0);
        NVIC::unmask(Interrupt::SPI0_IRQ);
        NVIC::unmask(Interrupt::UART0_IRQ);
        NVIC::unmask(Interrupt::DMA_IRQ_0);
        NVIC::unmask(Interrupt::TIMER_IRQ_0);
    }

    let main_future = main_loop(
        program,
        spi_bus.acquire_spi(),
        pins.gpio8.into_push_pull_output(),
        scr,
        screen_driver,
        &mut output,
        &mut delay,
    );

    pin_mut!(main_future);
    let mut cm = Cassette::new(main_future);

    loop {
        match cm.poll_on() {
            Some(o) => {
                match o {
                    Ok(()) => {
                        error!("This shouldn't happen!");
                    },
                    Err(e) => {
                        let s = format!("The error: {:?}", e);
                        error!("{}", s);
                    },
                }
                
            }
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
