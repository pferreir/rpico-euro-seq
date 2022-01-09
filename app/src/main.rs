//! Blinks the LED on a Pico board
//!
//! This will blink an LED attached to GP25, which is the pin the Pico uses for the on-board LED.

#![no_main]
#![no_std]

extern crate panic_halt;

mod encoder;
mod gate_cv;
mod midi_in;
mod programs;
mod screen;
mod switches;
mod ui;
mod util;

use core::{convert::Into, ops::DerefMut};

use cassette::{pin_mut, Cassette};
use cortex_m::{
    interrupt::{free, Mutex},
    singleton,
};
use cortex_m_rt::entry;
use defmt::*;
use defmt_rtt as _;

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

use crate::programs::Program;
use screen::{Screen, ScreenDriverWithPins};

#[link_section = ".boot2"]
#[no_mangle]
#[used]
pub static BOOT2_FIRMWARE: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;

pub static TIMER: Mutex<Option<Timer>> = Mutex::new(None);

async fn main_loop(
    program: &mut impl Program,
    scr: &mut Screen,
    mut screen_driver: &mut ScreenDriverWithPins,
    output: &mut gate_cv::GaveCVOutWithPins,
    timer: &Timer,
    delay: &mut cortex_m::delay::Delay
) -> ! {

    let buffer_addr = unsafe { scr.buffer_addr() };

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
                    program.process_ui_input(&msg)
                }
            }
        });
        free(|cs| {
            if let Some(switches) = switches::SWITCHES.borrow(cs).borrow_mut().deref_mut() {
                for msg in switches.iter_messages() {
                    program.process_ui_input(&msg)
                }
            }
        });

        let prog_time = ((timer.get_counter() / 1000) & 0xffffffff) as u32;
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
    let timer = Timer::new(pac.TIMER, &mut pac.RESETS);

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
    pac.RESETS.reset.modify(|_, w| {
        w.dma().clear_bit()
    });
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

    let (scr, screen_driver) = singleton!(: (Screen, ScreenDriverWithPins) = screen::init_screen(
        spi,
        &mut delay,
        screen_pins
    )).unwrap();

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

    let mut program = programs::DebugProgram::new();

    encoder::init_encoder(
        pins.gpio21.into_floating_input(),
        pins.gpio22.into_floating_input(),
        pins.gpio0.into_floating_input(),
    );

    switches::init_switches(
        pins.gpio2.into_pull_up_input(),
        pins.gpio3.into_pull_up_input(),
    );

    init_interrupts();

    unsafe {
        // enable edges in GPIO21 and GPIO22
        NVIC::unmask(Interrupt::IO_IRQ_BANK0);
        NVIC::unmask(Interrupt::SPI0_IRQ);
        NVIC::unmask(Interrupt::UART0_IRQ);
        NVIC::unmask(Interrupt::DMA_IRQ_0);
    }

    let main_future = main_loop(&mut program, scr, screen_driver, &mut output, &timer, &mut delay);

    pin_mut!(main_future);
    let mut cm = Cassette::new(main_future);

    cm.poll_on();

    loop {}
}

fn init_interrupts() {
    let mut pac = unsafe { Peripherals::steal() };
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
