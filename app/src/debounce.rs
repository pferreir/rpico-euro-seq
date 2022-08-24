use core::{cell::RefCell, fmt::Debug};

use critical_section::{Mutex, CriticalSection, with};
use embassy_time::{Timer, Duration};
use defmt::{error, trace};
use rp2040_hal::pac::Peripherals;

use crate::DEBOUNCE_SENDER;

pub struct DebounceCallback(pub fn(CriticalSection, &mut Peripherals));

impl Debug for DebounceCallback {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("DebounceCallback")
    }
}

pub fn call_debouncer(pac: &mut Peripherals, num_slice: u8, num_pin: u8, callback: DebounceCallback) {
    trace!("Disabling interrupt for {}:{}", num_slice, num_pin);

    with(|cs| {
        // disable interrupt
        pac.IO_BANK0.proc0_inte[num_slice as usize].modify(|r, w| {
            let v = r.bits();
            let bit_pos = num_pin * 4 + 2;
            trace!("DB {:b} -> {:b}", v, v & !(0x3u32 << bit_pos));
            unsafe { w.bits(v & !(0x3u32 << bit_pos)) }
        });

        let mut rm = DEBOUNCE_SENDER.borrow(cs).borrow_mut();
        let sender = rm.as_mut().unwrap();
        sender.try_send((num_slice, num_pin, callback)).unwrap();
    })
}

pub async fn debounce<'t>(
    num_slice: u8,
    num_pin: u8,
    callback: DebounceCallback,
) {
    let mut pac = unsafe { Peripherals::steal() };

    // clear pending ISRs
    pac.IO_BANK0.intr[num_slice as usize].modify(|r, w| {
        let v = r.bits();
        let bit_pos = num_pin * 4 + 2;
        unsafe { w.bits(v | (0x3u32 << bit_pos)) }
    });

    Timer::after(Duration::from_millis(10)).await;

    // enable back relevant interrupts
    pac.IO_BANK0.proc0_inte[num_slice as usize].modify(|r, w| {
        let v = r.bits();
        let bit_pos = num_pin * 4 + 2;
        trace!("CB {:b} -> {:b}", v, v | (0x3u32 << bit_pos));
        unsafe { w.bits(v | (0x3u32 << bit_pos)) }
    });

    with(|cs| {
        callback.0(cs, &mut pac);
    })
}
