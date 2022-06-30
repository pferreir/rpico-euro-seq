#![feature(alloc_error_handler)]

#![no_main]
#![no_std]

use core::alloc::Layout;

extern crate alloc;

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
use embedded_hal::spi::MODE_3;
use embedded_time::{fixed_point::FixedPoint, rate::Extensions};
use rp2040_hal as hal;

use hal::{
    clocks::{init_clocks_and_plls, Clock},
    pac,
    pac::{interrupt, Interrupt, Peripherals, NVIC},
    sio::Sio,
    watchdog::Watchdog,
    Spi, Timer,
};

use logic::{
    programs::{self, Program},
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

async fn main_loop(
    program: &mut impl Program,
    scr: &mut Framebuffer,
    mut screen_driver: &mut ScreenDriverWithPins,
    output: &mut gate_cv::GateCVOutWithPins,
    delay: &mut cortex_m::delay::Delay,
) -> ! {
    let buffer_addr = unsafe { scr.buffer_addr() };

    program.setup().await;

    loop {
        free(|cs| {
            if let Some(midi_in) = midi_in::MIDI_IN.borrow(cs).borrow_mut().deref_mut() {
                for msg in midi_in.iter_messages() {
                    program.process_midi(&msg)
                }
            }
        });
        free(|cs| {
            if let Some(encoder) = encoder::ROTARY_ENCODER.borrow(cs).borrow_mut().deref_mut() {
                for msg in encoder.iter_messages() {
                    program.process_ui_input(&msg);
                    // let mut s = String::<32>::new();
                    // uwrite!(s, "{:#?}", msg);
                    // info!("{}", s);
                }
            }
        });
        let prog_time = free(|cs| {
            if let Some(switches) = switches::SWITCHES.borrow(cs).borrow_mut().deref_mut() {
                for msg in switches.iter_messages() {
                    program.process_ui_input(&msg);
                    // let mut s = String::<32>::new();
                    // uwrite!(s, "{:#?}", msg);
                    // info!("{}", s);
                }
            }

            if let Some(timer) = TIMER.borrow(cs).borrow().as_ref() {
                ((timer.get_counter() / 1000) & 0xffffffff) as u32
            } else {
                panic!("Can't get TIMER!")
            }
        });

        program.run(prog_time);

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

    let mut output = gate_cv::GateCVOut::new(
        &mut pac.RESETS,
        // DAC
        pac.SPI1,
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

    let main_future = main_loop(&mut program, scr, screen_driver, &mut output, &mut delay);

    pin_mut!(main_future);
    let mut cm = Cassette::new(main_future);

    loop {
        match cm.poll_on() {
            Some(_) => {
                panic!("This shouldn't happen!");
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
