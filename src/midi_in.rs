use core::marker::PhantomData;
use embedded_midi::{MidiIn as DriverMidiIn, MidiMessage};
use embedded_time::rate::{Baud, Hertz};
use rp2040_hal::pac::RESETS;
use rp2040_hal::uart::{
    DataBits, Enabled, StopBits, UartConfig, UartDevice, UartPeripheral,
};
use rp2040_hal::{
    gpio::{
        pin::{bank0::BankPinId, FunctionUart},
        Pin, PinId,
    },
    uart::ReadErrorType,
};
use ufmt::derive::uDebug;


#[derive(uDebug)]
pub enum Error {
    Overrun,
    Break,
    Parity,
    Framing,
}

pub struct MidiIn<D: UartDevice, RX> {
    driver: DriverMidiIn<UartPeripheral<Enabled, D>>,
    _rx_pin: PhantomData<RX>,
}

impl<D: UartDevice, RX: PinId + BankPinId> MidiIn<D, RX> {
    pub fn new(
        resets: &mut RESETS,
        device: D,
        _rx_pin: Pin<RX, FunctionUart>,
        frequency: Hertz,
    ) -> Self {
        let uart = UartPeripheral::new(device, resets)
            .enable(
                UartConfig {
                    baudrate: Baud::new(31250),
                    data_bits: DataBits::Eight,
                    stop_bits: StopBits::One,
                    parity: None,
                },
                frequency,
            )
            .unwrap();

        Self {
            driver: DriverMidiIn::new(uart),
            _rx_pin: PhantomData,
        }
    }

    pub fn read_block(&mut self) -> Result<MidiMessage, Error> {
        nb::block!(self.driver.read()).map_err(|e| match e {
            ReadErrorType::Overrun => Error::Overrun,
            ReadErrorType::Break => Error::Break,
            ReadErrorType::Parity => Error::Parity,
            ReadErrorType::Framing => Error::Framing,
        })
    }
}
