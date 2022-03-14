use core::cell::RefCell;

use cortex_m::interrupt::{CriticalSection, Mutex, free};
use defmt::{error, trace};
use embedded_time::{
    duration::{Extensions}
};

use rp2040_hal::{Timer, pac::Peripherals, timer::{Alarm0, Alarm1, Alarm2, Alarm3}};

use crate::TIMER;

static ALARM_POOL: Mutex<RefCell<Option<[AlarmSlot; 4]>>> = Mutex::new(RefCell::new(None));

#[derive(Copy, Clone)]
pub enum AlarmArgs {
    None,
    U8U8(u8, u8),
}

trait Alarm {
    fn wrap(self) -> AlarmWrapper;
}

impl Alarm for Alarm0 {
    fn wrap(self) -> AlarmWrapper {
        AlarmWrapper::Alarm0(self)
    }
}

impl Alarm for Alarm1 {
    fn wrap(self) -> AlarmWrapper {
        AlarmWrapper::Alarm1(self)
    }
}

impl Alarm for Alarm2 {
    fn wrap(self) -> AlarmWrapper {
        AlarmWrapper::Alarm2(self)
    }
}

impl Alarm for Alarm3 {
    fn wrap(self) -> AlarmWrapper {
        AlarmWrapper::Alarm3(self)
    }
}

enum AlarmWrapper {
    Alarm0(Alarm0),
    Alarm1(Alarm1),
    Alarm2(Alarm2),
    Alarm3(Alarm3),
}


macro_rules! fire_alarm {
    ($timer: ident, $alarm: expr, $time: expr) => {{
        $alarm.schedule($time.microseconds()).unwrap();
        $alarm.enable_interrupt($timer);
    }};
}

impl AlarmWrapper {
    fn fire(&mut self, timer: &mut Timer, time: u32) {
        match self {
            AlarmWrapper::Alarm0(a) => fire_alarm!(timer, a, time),
            AlarmWrapper::Alarm1(a) => fire_alarm!(timer, a, time),
            AlarmWrapper::Alarm2(a) => fire_alarm!(timer, a, time),
            AlarmWrapper::Alarm3(a) => fire_alarm!(timer, a, time),
        }
    }

    fn id(&self) -> u8 {
        match self {
            AlarmWrapper::Alarm0(_) => 0,
            AlarmWrapper::Alarm1(_) => 1,
            AlarmWrapper::Alarm2(_) => 2,
            AlarmWrapper::Alarm3(_) => 3,
        }
    }
}

struct AlarmSlot {
    alarm: AlarmWrapper,
    value: Option<(fn(&CriticalSection, &mut Peripherals, AlarmArgs, u8), AlarmArgs)>,
}

impl AlarmSlot {
    fn new<A: Alarm>(alarm: A) -> Self {
        Self {
            alarm: alarm.wrap(),
            value: None,
        }
    }

    fn is_free(&self) -> bool {
        self.value.is_none()
    }

    fn free(&mut self) -> (fn(&CriticalSection, &mut Peripherals, AlarmArgs, u8), AlarmArgs) {
        self.value.take().unwrap()
    }

    fn set(&mut self, f: fn(&CriticalSection, &mut Peripherals, AlarmArgs, u8), args: AlarmArgs) {
        self.value = Some((f, args));
    }
}

pub fn init_interrupts(pac: &mut Peripherals, timer: &mut Timer) {
    free(|cs| {
        let data = ALARM_POOL.borrow(cs);
        data.replace(Some([
            AlarmSlot::new(timer.alarm_0().unwrap()),
            AlarmSlot::new(timer.alarm_1().unwrap()),
            AlarmSlot::new(timer.alarm_2().unwrap()),
            AlarmSlot::new(timer.alarm_3().unwrap()),
        ]));
    })
}

pub fn handle_irq(cs: &CriticalSection, pac: &mut Peripherals) {
    trace!("--- TIMER_IRQ ---");

    let ints = pac.TIMER.ints.read().bits();

    for n in 0..4 {
        // check ALARM_0..3 for interrupt flags
        if ints & (1 << n) > 0 {
            let mut alarm_pool = ALARM_POOL.borrow(cs).borrow_mut();
            if let Some(pool) = alarm_pool.as_mut() {
                if !pool[n].is_free() {
                    // make space for an alarm
                    let (f, args) = pool[n].free();

                    // call alarm handler
                    f(cs, pac, args, n as u8);

                    // acknowledge interrupt
                    pac.TIMER
                        .intr
                        .modify(|r, w| unsafe { w.bits(r.bits() & (1 << n)) });
                } else {
                    error!("Weird. The alarm slot should have something!");
                }
            } else {
                panic!("Can't access ALARM_POOL!")
            }
        }
    }
}

pub fn fire_alarm(
    cs: &CriticalSection,
    time: u32,
    callback: fn(&CriticalSection, &mut Peripherals, AlarmArgs, u8),
    args: AlarmArgs,
) -> u8 {
    let mut alarm_pool = ALARM_POOL.borrow(cs).borrow_mut();
    let mut singleton = TIMER.borrow(cs).borrow_mut();
    let timer = singleton.as_mut().unwrap();
    if let Some(pool) = alarm_pool.as_mut() {
        if let Some(slot) = pool.iter_mut().find(|s| s.is_free()) {
            slot.set(callback, args);
            slot.alarm.fire(timer, time);
            slot.alarm.id()
        } else {
            panic!("Ran out of alarm slots");
        }
    } else {
        panic!("Can't get hold of ALARM_POOL!")
    }
}
