//! Blinks the LED on a Pico board
//!
//! This will blink an LED attached to GP25, which is the pin the Pico uses for the on-board LED.
#![no_std]
#![no_main]

extern crate nb;

use cortex_m_rt::entry;
use defmt::*;
use defmt_rtt as _;
use display_interface_spi::SPIInterfaceNoCS;
use embedded_graphics::{
    draw_target::DrawTarget,
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    prelude::{Point, RgbColor},
    text::Text,
};
use embedded_hal::{
    digital::v2::{InputPin, IoPin, OutputPin},
    spi::{MODE_0, MODE_3},
};
use embedded_midi::MidiIn;
use embedded_time::{
    fixed_point::FixedPoint,
    rate::{Baud, Extensions},
};
use heapless::String;
use mcp49xx::{Channel, Command as DacCommand, Mcp49xx};
use panic_probe as _;
use rp2040_hal as hal;

use hal::{
    clocks::{init_clocks_and_plls, Clock},
    pac,
    sio::Sio,
    uart::UartConfig,
    watchdog::Watchdog,
};

use st7789::ST7789;

#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;

#[entry]
fn main() -> ! {
    info!("Program start");
    let mut pac = pac::Peripherals::take().unwrap();
    let core = pac::CorePeripherals::take().unwrap();
    let mut watchdog = Watchdog::new(pac.WATCHDOG);
    let sio = Sio::new(pac.SIO);

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

    let mut backlight = pins.gpio15.into_push_pull_output();
    let rst = pins.gpio14.into_push_pull_output();
    let dc = pins.gpio13.into_push_pull_output();
    let _clk0 = pins.gpio18.into_mode::<hal::gpio::FunctionSpi>();
    let _mosi0 = pins.gpio19.into_mode::<hal::gpio::FunctionSpi>();

    backlight.set_high().unwrap();

    let spi0 = hal::spi::Spi::<_, _, 8>::new(pac.SPI0).init(
        &mut pac.RESETS,
        125_000_000u32.Hz(),
        16_000_000u32.Hz(),
        &MODE_3,
    );

    let di = SPIInterfaceNoCS::new(spi0, dc);
    let mut screen = ST7789::new(di, rst, 240, 240);

    let _midi_pin = pins.gpio1.into_mode::<hal::gpio::FunctionUart>();
    let uart = hal::uart::UartPeripheral::<_, _>::new(pac.UART0, &mut pac.RESETS)
        .enable(
            UartConfig {
                baudrate: Baud::new(31250),
                data_bits: hal::uart::DataBits::Eight,
                stop_bits: hal::uart::StopBits::One,
                parity: None,
            },
            clocks.peripheral_clock.into(),
        )
        .unwrap();

    let _clk1 = pins.gpio10.into_mode::<hal::gpio::FunctionSpi>();
    let _mosi1 = pins.gpio11.into_mode::<hal::gpio::FunctionSpi>();
    let cs = pins.gpio9.into_push_pull_output();

    let spi1 = hal::spi::Spi::<_, _, 8>::new(pac.SPI1).init(
        &mut pac.RESETS,
        125_000_000u32.Hz(),
        1_000_000u32.Hz(),
        &MODE_0,
    );

    let mut dac = Mcp49xx::new_mcp4822(spi1, cs);

    let mut midi_in = MidiIn::new(uart);

    // let raw_image_data = ImageRawLE::<Rgb565>::new(include_bytes!("../assets/ferris.raw"), 86);
    // let ferris = Image::new(&raw_image_data, Point::new(34, 8));

    screen.init(&mut delay).unwrap();
    screen
        .set_orientation(st7789::Orientation::Portrait)
        .unwrap();
    screen.clear(Rgb565::BLUE).unwrap();
    // ferris.draw(&mut screen).unwrap();

    let style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);

    Text::new("HELLOz", Point::new(20, 20), style)
        .draw(&mut screen)
        .unwrap();

    let mut line_no = 2;

    let cmd = DacCommand::default();
    let cmd = cmd.double_gain();
    dac.send(cmd).unwrap();
    let cmd = cmd.channel(Channel::Ch1).value(0xfff);
    dac.send(cmd).unwrap();
    let cmd = cmd.channel(Channel::Ch0).value(0x7ff);
    dac.send(cmd).unwrap();

    let encoder_1 = pins.gpio21.into_floating_input();
    let encoder_2 = pins.gpio22.into_floating_input();
    let encoder_sw = pins.gpio0.into_floating_input();

    let mut e1 = encoder_1.is_high().unwrap();
    let mut e2 = encoder_2.is_high().unwrap();
    let mut esw = encoder_sw.is_high().unwrap();

    let sw1 = pins.gpio2.into_pull_up_input();
    let sw2 = pins.gpio3.into_pull_up_input();

    let mut trig1 = pins.gpio4.into_push_pull_output();
    let mut trig2 = pins.gpio5.into_push_pull_output();

    let mut s1 = sw1.is_high().unwrap();
    let mut s2 = sw2.is_high().unwrap();

    let mut t_state = false;

    trig1.set_high().unwrap();
    trig2.set_low().unwrap();

    loop {
        let mut s = String::<16>::new();
        match nb::block!(midi_in.read()) {
            Ok(event) => {
                s.push_str(match event {
                    embedded_midi::MidiMessage::NoteOff(_, _, _) => "NoteOff",
                    embedded_midi::MidiMessage::NoteOn(_, _, _) => "NoteOn",
                    _ => "Whatever",
                })
                .unwrap();
            }
            Err(e) => {
                s.push_str(match e {
                    hal::uart::ReadErrorType::Overrun => "Overrun",
                    hal::uart::ReadErrorType::Break => "Break",
                    hal::uart::ReadErrorType::Parity => "Parity",
                    hal::uart::ReadErrorType::Framing => "Framing",
                })
                .unwrap();
            }
        }

        Text::new(&s, Point::new(20, line_no * 15), style)
            .draw(&mut screen)
            .unwrap();
        line_no += 1;

        // let cur_e1 = encoder_1.is_high().unwrap();
        // if e1 != cur_e1 {
        //     Text::new("E1", Point::new(20, line_no * 15), style)
        //         .draw(&mut screen)
        //         .unwrap();
        //     line_no += 1;
        //     e1 = cur_e1;
        // }
        // let cur_e2 = encoder_2.is_high().unwrap();
        // if e2 != cur_e2 {
        //     Text::new("E2", Point::new(20, line_no * 15), style)
        //         .draw(&mut screen)
        //         .unwrap();
        //     line_no += 1;
        //     e2 = cur_e2;
        // }
        // let cur_esw = encoder_sw.is_high().unwrap();
        // if esw != cur_esw {
        //     Text::new("ESW", Point::new(20, line_no * 15), style)
        //         .draw(&mut screen)
        //         .unwrap();
        //     line_no += 1;
        //     esw = cur_esw;

        //     if t_state {
        //         trig1.set_high().unwrap();
        //         trig2.set_low().unwrap();
        //     } else {
        //         trig1.set_low().unwrap();
        //         trig2.set_high().unwrap();
        //     }
        //     t_state = !t_state;
        // }
        // let cur_s1 = sw1.is_high().unwrap();
        // if s1 != cur_s1 {
        //     Text::new("s1", Point::new(20, line_no * 15), style)
        //         .draw(&mut screen)
        //         .unwrap();
        //     line_no += 1;
        //     s1 = cur_s1;
        // }
        // let cur_s2 = sw2.is_high().unwrap();
        // if s2 != cur_s2 {
        //     Text::new("s2", Point::new(20, line_no * 15), style)
        //         .draw(&mut screen)
        //         .unwrap();
        //     line_no += 1;
        //     s2 = cur_s2;
        // }
    }
}
