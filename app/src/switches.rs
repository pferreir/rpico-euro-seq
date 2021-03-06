use core::{cell::RefCell, ops::DerefMut};

use cortex_m::interrupt::{free, CriticalSection, Mutex};
use embedded_hal::digital::v2::InputPin;
use heapless::spsc::Queue;
use rp2040_hal::{
    gpio::{
        pin::bank0::{BankPinId, Gpio2, Gpio3},
        Pin, PinId, PullUpInput,
    },
    pac::Peripherals,
};

use crate::debounce::debounce;
use logic::{ui::UIInputEvent, util::QueuePoppingIter};

const DEBOUNCE_INTERVAL: u32 = 10000;

pub static SWITCHES: Mutex<RefCell<Option<Switches<Gpio2, Gpio3>>>> =
    Mutex::new(RefCell::new(None));

pub struct Switches<SW1: PinId + BankPinId, SW2: PinId + BankPinId> {
    sw1: Pin<SW1, PullUpInput>,
    sw2: Pin<SW2, PullUpInput>,
    sw1_last_state: bool,
    sw2_last_state: bool,
    event_queue: Queue<UIInputEvent, 32>,
}

impl<SW1: PinId + BankPinId, SW2: PinId + BankPinId> Switches<SW1, SW2> {
    fn new(sw1: Pin<SW1, PullUpInput>, sw2: Pin<SW2, PullUpInput>) -> Self {
        Self {
            sw1,
            sw2,
            sw1_last_state: false,
            sw2_last_state: false,
            event_queue: Queue::new(),
        }
    }

    fn refresh_switches(&mut self) {
        let sw1_high = self.sw1.is_high().unwrap();
        let sw2_high = self.sw2.is_high().unwrap();

        if self.sw1_last_state != sw1_high {
            self.event_queue
                .enqueue(UIInputEvent::Switch1(self.sw1_last_state))
                .unwrap();
        }
        if self.sw2_last_state != sw2_high {
            self.event_queue
                .enqueue(UIInputEvent::Switch2(self.sw2_last_state))
                .unwrap();
        }

        self.sw1_last_state = sw1_high;
        self.sw2_last_state = sw2_high;
    }

    pub fn iter_messages<'t>(&'t mut self) -> impl Iterator<Item = UIInputEvent> + 't {
        QueuePoppingIter::new(&mut self.event_queue)
    }
}

fn handle_switch_interrupt(cs: &CriticalSection, pac: &mut Peripherals) {
    if let Some(ref mut switches) = SWITCHES.borrow(cs).borrow_mut().deref_mut() {
        switches.refresh_switches();
    }
}

pub fn init_switches(sw1: Pin<Gpio2, PullUpInput>, sw2: Pin<Gpio3, PullUpInput>) {
    free(|cs| {
        SWITCHES.borrow(cs).replace(Some(Switches::new(sw1, sw2)));
    });
}

pub fn init_interrupts(pac: &mut Peripherals) {
    // set edge interrupts
    pac.IO_BANK0.proc0_inte[0].modify(|_, w| {
        // GPIO2
        w.gpio2_edge_high().set_bit();
        w.gpio2_edge_low().set_bit();
        // GPIO3
        w.gpio3_edge_high().set_bit();
        w.gpio3_edge_low().set_bit()
    });
}

pub fn handle_irq(cs: &CriticalSection, pac: &mut Peripherals) {
    let reg_r = pac.IO_BANK0.intr[0].read();

    if reg_r.gpio2_edge_high().bit() {
        debounce(cs, pac, 0, 2, DEBOUNCE_INTERVAL, handle_switch_interrupt);
    }
    if reg_r.gpio2_edge_low().bit() {
        debounce(cs, pac, 0, 2, DEBOUNCE_INTERVAL, handle_switch_interrupt);
    }

    if reg_r.gpio3_edge_high().bit() {
        debounce(cs, pac, 0, 3, DEBOUNCE_INTERVAL, handle_switch_interrupt);
    }
    if reg_r.gpio3_edge_low().bit() {
        debounce(cs, pac, 0, 3, DEBOUNCE_INTERVAL, handle_switch_interrupt);
    }
}
