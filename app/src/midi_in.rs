use core::cell::RefCell;
use cortex_m::interrupt::{free, CriticalSection, Mutex};
use embedded_midi::{MidiIn as DriverMidiIn, MidiMessage};
use embedded_time::rate::{Baud, Hertz};
use heapless::spsc::Queue;
use rp2040_hal::pac::{Peripherals, RESETS, UART0};
use rp2040_hal::uart::{DataBits, Enabled, Rx, StopBits, UartConfig, UartDevice, UartPeripheral};
use rp2040_hal::{
    gpio::{
        pin::{
            bank0::{BankPinId, Gpio1},
            FunctionUart,
        },
        Pin, PinId,
    },
    uart::ReadErrorType,
};
use ufmt::derive::uDebug;

use crate::util::QueuePoppingIter;

pub static MIDI_IN: Mutex<RefCell<Option<MidiIn<UART0, Gpio1>>>> = Mutex::new(RefCell::new(None));

#[derive(uDebug)]
pub enum Error {
    Overrun,
    Break,
    Parity,
    Framing,
}

pub struct MidiIn<D: UartDevice, RX: PinId + BankPinId>
where
    Pin<RX, FunctionUart>: Rx<D>,
{
    driver: DriverMidiIn<UartPeripheral<Enabled, D, ((), Pin<RX, FunctionUart>)>>,
    queue: Queue<MidiMessage, 16>,
}

fn process_error(e: ReadErrorType) -> Error {
    match e {
        ReadErrorType::Overrun => Error::Overrun,
        ReadErrorType::Break => Error::Break,
        ReadErrorType::Parity => Error::Parity,
        ReadErrorType::Framing => Error::Framing,
    }
}

impl<D: UartDevice, RX: PinId + BankPinId> MidiIn<D, RX>
where
    Pin<RX, FunctionUart>: Rx<D>,
{
    pub fn new(uart: UartPeripheral<Enabled, D, ((), Pin<RX, FunctionUart>)>) -> Self {
        Self {
            driver: DriverMidiIn::new(uart),
            queue: Queue::new(),
        }
    }

    pub fn read_message(&mut self) {
        loop {
            match self.driver.read() {
                Ok(msg) => match self.queue.enqueue(msg) {
                    Ok(()) => {}
                    Err(rej_msg) => {
                        self.queue.dequeue();
                        unsafe { self.queue.enqueue_unchecked(rej_msg) };
                    }
                },
                Err(e) => match e {
                    nb::Error::Other(err) => panic!(),
                    nb::Error::WouldBlock => break,
                },
            };
        }
    }

    pub fn iter_messages<'t>(&'t mut self) -> impl Iterator<Item = MidiMessage> + 't {
        QueuePoppingIter::new(&mut self.queue)
    }
}

pub fn init_midi_in(
    resets: &mut RESETS,
    device: UART0,
    rx: Pin<Gpio1, FunctionUart>,
    periph_frequency: Hertz,
) {
    let uart = UartPeripheral::new(device, ((), rx), resets)
        .enable(
            UartConfig {
                baudrate: Baud::new(31250),
                data_bits: DataBits::Eight,
                stop_bits: StopBits::One,
                parity: None
            },
            periph_frequency,
        )
        .unwrap();
    let midi_in = MidiIn::new(uart);
    free(|cs| {
        let mut singleton = MIDI_IN.borrow(cs).borrow_mut();
        singleton.replace(midi_in);
    });
}

pub fn init_interrupts(pac: &mut Peripherals) {
    // set RX interrupt on UART0
    pac.UART0.uartimsc.modify(|_, w| {
        w.rxim().set_bit();
        w.rtim().set_bit()
    });
    unsafe { pac.UART0.uartifls.modify(|_, w| w.rxiflsel().bits(0)) };
}

pub fn handle_irq(cs: &CriticalSection, pac: &mut Peripherals) {
    let r = pac.UART0.uartmis.read();
    if !r.rxmis().bit_is_set() && !r.rtmis().bit_is_set() {
        return;
    }

    if let Some(ref mut midi_in) = MIDI_IN.borrow(cs).borrow_mut().as_mut() {
        midi_in.read_message();
    }

    // no need to clear IRQs, since reading from the UART buffer
    // does it
}
