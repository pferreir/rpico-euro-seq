use core::{cell::RefCell, marker::PhantomData, ops::DerefMut};

use cortex_m::interrupt::{free, Mutex, CriticalSection};
use rp2040_hal::{gpio::{
    pin::bank0::{BankPinId, Gpio2, Gpio3},
    Pin, PinId, Input, PullUp,
}, pac::Peripherals};

pub static SWITCHES: Mutex<RefCell<Option<Switches<Gpio2, Gpio3>>>> = Mutex::new(RefCell::new(None));

pub enum Switch {
    SW1,
    SW2
}
pub struct Switches<SW1: PinId + BankPinId, SW2: PinId + BankPinId> {
    _sw1: PhantomData<SW1>,
    _sw2: PhantomData<SW2>,
    sw1: bool,
    sw2: bool
}

impl<SW1: PinId + BankPinId, SW2: PinId + BankPinId> Switches<SW1, SW2> {
    fn new(_sw1: Pin<SW1, Input<PullUp>>, _sw2: Pin<SW2, Input<PullUp>>) -> Self {
        Self {
            sw1: false,
            sw2: false,
            _sw1: PhantomData,
            _sw2: PhantomData
        }
    }

    fn handle_switch(&mut self, switch: Switch, state: bool) {
        match switch {
            Switch::SW1 => self.sw1 = state,
            Switch::SW2 => self.sw2 = state,
        }
    }

    pub fn switches(&self) -> (bool, bool) {
        (self.sw1, self.sw2)
    }
}

fn handle_switch_interrupt(cs: &CriticalSection, switch: Switch, state: bool) {
    if let Some(ref mut switches) = SWITCHES.borrow(cs).borrow_mut().deref_mut() {
        switches.handle_switch(switch, state);
    }
}

pub fn init_switches(sw1: Pin<Gpio2, Input<PullUp>>, sw2: Pin<Gpio3, Input<PullUp>>) {
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
        handle_switch_interrupt(cs, Switch::SW1, false);
        pac.IO_BANK0.intr[0].write(|w| w.gpio2_edge_high().set_bit());
    }
    if reg_r.gpio2_edge_low().bit() {
        handle_switch_interrupt(cs, Switch::SW1, true);
        pac.IO_BANK0.intr[0].write(|w| w.gpio2_edge_low().set_bit());
    }

    if reg_r.gpio3_edge_high().bit() {
        handle_switch_interrupt(cs, Switch::SW2, false);
        pac.IO_BANK0.intr[0].write(|w| w.gpio3_edge_high().set_bit());
    }
    if reg_r.gpio3_edge_low().bit() {
        handle_switch_interrupt(cs, Switch::SW2, true);
        pac.IO_BANK0.intr[0].write(|w| w.gpio3_edge_low().set_bit());
    }
}
