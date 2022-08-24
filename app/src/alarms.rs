use core::cell::{Cell, RefMut, RefCell};
use atomic_polyfill::{AtomicU8, Ordering};
use critical_section::{CriticalSection, with};
use defmt::trace;
use embassy_time::{driver::{Driver, AlarmHandle}, TICKS_PER_SECOND};
use embassy_sync::blocking_mutex::{raw::CriticalSectionRawMutex, Mutex};
use embedded_time::duration::Microseconds;
use rp2040_hal::{
    pac::Peripherals,
    timer::{Alarm as AlarmTrait, Alarm0, Alarm1, Alarm2, Alarm3, ScheduleAlarmError},
    Timer,
};

struct AlarmSlot {
    timestamp: Cell<u64>,
    callback: Cell<Option<(fn(*mut ()), *mut ())>>,
}
unsafe impl Send for AlarmSlot {}

struct TimerDriver {
    timer: Mutex<CriticalSectionRawMutex, RefCell<Option<Timer>>>,
    alarms: Mutex<CriticalSectionRawMutex, RefCell<Option<[AlarmWrapper; ALARM_COUNT]>>>,
    alarm_slots: Mutex<CriticalSectionRawMutex, [AlarmSlot; ALARM_COUNT]>,
    next_alarm: AtomicU8,
}

const ALARM_COUNT: usize = 4;
const DUMMY_ALARM: AlarmSlot = AlarmSlot {
    timestamp: Cell::new(0),
    callback: Cell::new(None)
};

embassy_time::time_driver_impl!(static DRIVER: TimerDriver = TimerDriver{
    alarm_slots:  Mutex::const_new(CriticalSectionRawMutex::new(), [DUMMY_ALARM; ALARM_COUNT]),
    alarms: Mutex::const_new(CriticalSectionRawMutex::new(), RefCell::new(None)),
    next_alarm: AtomicU8::new(0),
    timer: Mutex::const_new(CriticalSectionRawMutex::new(), RefCell::new(None)),
});

pub fn now() -> u64 {
    DRIVER.now()
}

impl Driver for TimerDriver {
    fn now(&self) -> u64 {
        with(|cs| {
            self.timer.borrow(cs).borrow().as_ref().unwrap().get_counter()
        })
    }

    unsafe fn allocate_alarm(&self) -> Option<AlarmHandle> {
        let id = self.next_alarm.fetch_update(Ordering::AcqRel, Ordering::Acquire, |x| {
            if x < ALARM_COUNT as u8 {
                Some(x + 1)
            } else {
                None
            }
        });

        match id {
            Ok(id) => Some(AlarmHandle::new(id)),
            Err(_) => None,
        }
    }

    fn set_alarm_callback(&self, alarm: AlarmHandle, callback: fn(*mut ()), ctx: *mut ()) {
        let n = alarm.id() as usize;
        critical_section::with(|cs| {
            let alarm = &self.alarm_slots.borrow(cs)[n];
            alarm.callback.set(Some((callback, ctx)));
        })
    }

    fn set_alarm(&self, alarm: embassy_time::driver::AlarmHandle, timestamp: u64) {
        let n = alarm.id() as usize;
        critical_section::with(|cs| {
            let mut rm = self.alarms.borrow(cs).borrow_mut();
            let alarms = rm.as_mut().unwrap();
            let alarm_slot = &self.alarm_slots.borrow(cs)[n];
            alarm_slot.timestamp.set(timestamp);

            let now = self.now();

            // trace!("arm {} - timestamp: {} - now: {}", n, timestamp, now);
            // Arm it.
            // Note that we're not checking the high bits at all. This means the irq may fire early
            // if the alarm is more than 72 minutes (2^32 us) in the future. This is OK, since on irq fire
            // it is checked if the alarm time has passed.
            alarms[n].schedule((timestamp - now) as u32).unwrap();

            // If alarm timestamp has passed, trigger it instantly.
            // This disarms it.
            if timestamp <= now {
                self.trigger_alarm(n, cs, unsafe { &Peripherals::steal() });
            }
        })
    }
}

#[derive(Copy, Clone)]
pub enum AlarmArgs {
    None,
    U8U8(u8, u8),
}

impl TimerDriver {
    fn check_alarm(&self, n: usize, pac: &Peripherals) {
        trace!("checking alarm {}", n);
        critical_section::with(|cs| {
            let timestamp = self.alarm_slots.borrow(cs)[n].timestamp.get();
            let mut rm = self.alarms.borrow(cs).borrow_mut();
            let alarms = rm.as_mut().unwrap();
            let now = self.now();

            if timestamp <= now {
                self.trigger_alarm(n, cs, pac)
            } else {
                // Not elapsed, arm it again.
                // This can happen if it was set more than 2^32 us in the future.
                alarms[n].schedule((timestamp - now) as u32).unwrap();
            }

            // clear the irq
            alarms[n].clear_interrupt();

        });
    }

    fn trigger_alarm(&self, n: usize, cs: CriticalSection, pac: &Peripherals) {
        // disarm alarm
        pac.TIMER.armed.modify(|r, w| {
            unsafe { w.bits(r.bits() & (1 << n)) }
        });

        let alarm = &self.alarm_slots.borrow(cs)[n];
        alarm.timestamp.set(u64::MAX);

        // Call after clearing alarm, so the callback can set another alarm.
        if let Some((f, ctx)) = alarm.callback.get() {
            f(ctx);
        }
    }
}


enum AlarmWrapper {
    Alarm0(Alarm0),
    Alarm1(Alarm1),
    Alarm2(Alarm2),
    Alarm3(Alarm3),
}

impl AlarmWrapper {
    fn schedule(&mut self, time: u32) -> Result<(), ScheduleAlarmError> {
        // trace!("scheduling alarm in {} us", time);
        let time: Microseconds = Microseconds(time);
        match self {
            AlarmWrapper::Alarm0(a) => a.schedule(time),
            AlarmWrapper::Alarm1(a) => a.schedule(time),
            AlarmWrapper::Alarm2(a) => a.schedule(time),
            AlarmWrapper::Alarm3(a) => a.schedule(time),
        }
    }

    fn enable_interrupt(&mut self) {
        match self {
            AlarmWrapper::Alarm0(a) => a.enable_interrupt(),
            AlarmWrapper::Alarm1(a) => a.enable_interrupt(),
            AlarmWrapper::Alarm2(a) => a.enable_interrupt(),
            AlarmWrapper::Alarm3(a) => a.enable_interrupt(),
        }
    }

    fn clear_interrupt(&mut self) {
        match self {
            AlarmWrapper::Alarm0(a) => a.clear_interrupt(),
            AlarmWrapper::Alarm1(a) => a.clear_interrupt(),
            AlarmWrapper::Alarm2(a) => a.clear_interrupt(),
            AlarmWrapper::Alarm3(a) => a.clear_interrupt(),
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

pub fn init_interrupts(mut timer: Timer) {
    with(|cs| {
        let alarm_slots = DRIVER.alarm_slots.borrow(cs);
        for a in alarm_slots {
            a.timestamp.set(u64::MAX);
        }
    });


    let mut alarms = [
        AlarmWrapper::Alarm0(timer.alarm_0().unwrap()),
        AlarmWrapper::Alarm1(timer.alarm_1().unwrap()),
        AlarmWrapper::Alarm2(timer.alarm_2().unwrap()),
        AlarmWrapper::Alarm3(timer.alarm_3().unwrap()),
    ];
    for alarm in &mut alarms {
        alarm.enable_interrupt();
    }
    with(|cs| {
        DRIVER.timer.borrow(cs).borrow_mut().replace(timer);
        DRIVER.alarms.borrow(cs).borrow_mut().replace(alarms);
    });
}

pub fn handle_irq(n: usize, cs: CriticalSection, pac: &mut Peripherals) {
    trace!("--- TIMER_IRQ {} ---", n);
    DRIVER.check_alarm(n, pac)
}

