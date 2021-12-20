//! Blinks the LED on a Pico board
//!
//! This will blink an LED attached to GP25, which is the pin the Pico uses for the on-board LED.
#![no_std]
#![no_main]

extern crate nb;
extern crate ufmt;

mod dac;
mod encoder;
mod midi_in;
mod screen;

use core::ops::DerefMut;

use cortex_m::{
    interrupt::{free, Mutex},
    singleton,
};
use cortex_m_rt::entry;
use defmt::*;
use defmt_rtt as _;
use panic_probe as _;

use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    prelude::{Point, RgbColor},
    text::Text,
};
use embedded_hal::{digital::v2::OutputPin, spi::MODE_3};
use embedded_time::{fixed_point::FixedPoint, rate::Extensions};
use heapless::String;
use rp2040_hal as hal;
use ufmt::uwrite;

use hal::{
    clocks::{init_clocks_and_plls, Clock},
    dma::{DMAExt, SingleBufferingConfig},
    pac,
    pac::{interrupt, Interrupt, Peripherals, NVIC},
    sio::Sio,
    watchdog::Watchdog,
    Spi, Timer,
};

#[link_section = ".boot2"]
#[no_mangle]
#[used]
pub static BOOT2_FIRMWARE: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;

pub static TIMER: Mutex<Option<Timer>> = Mutex::new(None);

#[entry]
fn main() -> ! {
    info!("Program start");
    let mut pac = pac::Peripherals::take().unwrap();
    let core = pac::CorePeripherals::take().unwrap();
    let mut watchdog = Watchdog::new(pac.WATCHDOG);
    let sio = Sio::new(pac.SIO);
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

    let mut delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().integer());

    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    // Initialize DMA.
    let dma = pac.DMA.split(&mut pac.RESETS);

    let spi = Spi::<_, _, 8>::new(pac.SPI0).init(
        &mut pac.RESETS,
        125_000_000u32.Hz(),
        16_000_000u32.Hz(),
        &MODE_3,
    );

    let mut screen = screen::init_screen(
        dma.ch0,
        spi,
        &mut delay,
        pins.gpio18.into_mode::<hal::gpio::FunctionSpi>(),
        pins.gpio19.into_mode::<hal::gpio::FunctionSpi>(),
        pins.gpio14.into_push_pull_output(),
        pins.gpio13.into_push_pull_output(),
        pins.gpio15.into_push_pull_output(),
    );

    let mut midi_in = midi_in::MidiIn::new(
        &mut pac.RESETS,
        pac.UART0,
        pins.gpio1.into_mode::<hal::gpio::FunctionUart>(),
        clocks.peripheral_clock.into(),
    );

    let mut dac = dac::Dac::new(
        &mut pac.RESETS,
        pac.SPI1,
        pins.gpio10.into_mode::<hal::gpio::FunctionSpi>(),
        pins.gpio11.into_mode::<hal::gpio::FunctionSpi>(),
        pins.gpio9.into_push_pull_output(),
    );

    // let raw_image_data = ImageRawLE::<Rgb565>::new(include_bytes!("../assets/ferris.raw"), 86);
    // let ferris = Image::new(&raw_image_data, Point::new(34, 8));

    // ferris.draw(&mut screen).unwrap();

    let style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);

    dac.init();
    dac.set_ch1(0x0);

    encoder::init_encoder(
        pins.gpio21.into_floating_input(),
        pins.gpio22.into_floating_input(),
        pins.gpio0.into_floating_input(),
    );

    let sw1 = pins.gpio2.into_pull_up_input();
    let sw2 = pins.gpio3.into_pull_up_input();

    init_interrupts();

    unsafe {
        // enable edges in GPIO21 and GPIO22
        NVIC::unmask(Interrupt::IO_IRQ_BANK0);
        NVIC::unmask(Interrupt::SPI0_IRQ);
    }

    let mut trig1 = pins.gpio4.into_push_pull_output();
    let mut trig2 = pins.gpio5.into_push_pull_output();

    trig1.set_high().unwrap();
    trig2.set_low().unwrap();

    let mut frame_counter = 0u8;
    let mut last_tick = 0;
    let mut fps = 0;

    loop {
        let mut s = String::<16>::new();

        free(|cs| {
            if let Some(encoder) = encoder::ROTARY_ENCODER.borrow(cs).borrow_mut().deref_mut() {
                uwrite!(s, "=> {}", encoder.val).unwrap();
            }
        });
        // for counter in 3000..4095 {
        //     delay.delay_ms(1);

        //     dac.set_ch0(counter);
        // }
        // for counter in 0..1000 {
        //     delay.delay_ms(1);

        //     dac.set_ch0(4095 - counter);
        // }

        // match midi_in.read_block() {

        //     Ok(event) => {
        //         s.push_str(match event {
        //             embedded_midi::MidiMessage::NoteOff(_, _, _) => "NoteOff",
        //             embedded_midi::MidiMessage::NoteOn(_, _, _) => "NoteOn",
        //             _ => "Whatever",
        //         })
        //         .unwrap();
        //     }, Err(e) => {
        //         uwrite!(s, "{:?}", e).unwrap();
        //     }
        // }
        screen.clear(Rgb565::BLACK).unwrap();

        Text::new(&s, Point::new(20, 15), style)
        .draw(&mut screen)
        .unwrap();

        let diff = (timer.get_counter() - last_tick) as u32;
        if diff >= 1_000_000u32 {
            fps = frame_counter as u32 * 1_000_000 / diff;
            frame_counter = 0;
            last_tick = timer.get_counter();
        }

        s.truncate(0);
        uwrite!(s, "{} fps", fps).unwrap();

        Text::new(&s, Point::new(20, 55), style)
            .draw(&mut screen)
            .unwrap();

        screen.refresh();
        frame_counter += 1;
    }
}

fn init_interrupts() {
    let mut pac = unsafe { Peripherals::steal() };
    encoder::init_interrupts(&mut pac);
    screen::init_interrupts(&mut pac);
}

#[interrupt]
fn IO_IRQ_BANK0() {
    free(|cs| {
        let mut pac = unsafe { Peripherals::steal() };
        encoder::handle_irq(cs, &mut pac);
    });
}

#[interrupt]
fn SPI0_IRQ() {
    free(|cs| {
        let mut pac = unsafe { Peripherals::steal() };
        screen::handle_irq(cs, &mut pac);
    });
}
