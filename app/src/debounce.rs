use core::cell::RefCell;

use critical_section::{Mutex, CriticalSection};
use defmt::{error, trace};
use rp2040_hal::pac::Peripherals;

use crate::alarms::{fire_alarm, AlarmArgs};

static DEBOUNCE_CALLBACKS: Mutex<RefCell<[Option<fn(CriticalSection, &mut Peripherals)>; 4]>> =
    Mutex::new(RefCell::new([None; 4]));

pub fn debounce_callback(
    cs: CriticalSection,
    pac: &mut Peripherals,
    args: AlarmArgs,
    alarm_id: u8,
) {
    if let AlarmArgs::U8U8(num_slice, num_pin) = args {
        // clear any pending ISRs
        pac.IO_BANK0.intr[num_slice as usize].modify(|r, w| {
            let v = r.bits();
            let bit_pos = num_pin * 4 + 2;
            unsafe { w.bits(v | (0x3u32 << bit_pos)) }
        });

        // enable back relevant interrupts
        pac.IO_BANK0.proc0_inte[num_slice as usize].modify(|r, w| {
            let v = r.bits();
            let bit_pos = num_pin * 4 + 2;
            trace!("CB {:b} -> {:b}", v, v | (0x3u32 << bit_pos));
            unsafe { w.bits(v | (0x3u32 << bit_pos)) }
        });

        // callback
        let mut actions = DEBOUNCE_CALLBACKS.borrow(cs).borrow_mut();
        let cb = actions[alarm_id as usize].take();
        if let Some(f) = cb {
            f(cs, pac);
        } else {
            error!("Callback should have been registered!");
        }
    } else {
        panic!("Unexpected callback args")
    }
}

pub fn debounce(
    cs: CriticalSection,
    pac: &mut Peripherals,
    num_slice: u8,
    num_pin: u8,
    time: u32,
    callback: fn(CriticalSection, &mut Peripherals),
) {
    // disable interrupt
    pac.IO_BANK0.proc0_inte[num_slice as usize].modify(|r, w| {
        let v = r.bits();
        let bit_pos = num_pin * 4 + 2;
        trace!("DB {:b} -> {:b}", v, v & !(0x3u32 << bit_pos));
        unsafe { w.bits(v & !(0x3u32 << bit_pos)) }
    });

    let alarm_id = fire_alarm(
        cs,
        time,
        debounce_callback,
        AlarmArgs::U8U8(num_slice, num_pin),
    );

    let mut actions = DEBOUNCE_CALLBACKS.borrow(cs).borrow_mut();
    actions[alarm_id as usize].replace(callback);
}
