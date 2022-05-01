use core::{cell::RefCell, marker::PhantomData, ops::DerefMut};

use cortex_m::interrupt::{free, CriticalSection, Mutex};
use defmt::trace;
use heapless::spsc::Queue;
use rotary_encoder_embedded::{Direction, RotaryEncoder};
use rp2040_hal::{
    gpio::{
        pin::{
            bank0::{
                BankPinId, {Gpio0, Gpio21, Gpio22},
            },
            FloatingInput,
        },
        Pin, PinId,
    },
    pac::Peripherals,
};

use crate::debounce::debounce;
use logic::{ui::UIInputEvent, util::QueuePoppingIter};

const DEBOUNCE_INTERVAL: u32 = 10000;

fn update_turns<const N: usize>(queue: &mut Queue<UIInputEvent, N>, val: i8) {
    match queue.dequeue() {
        Some(UIInputEvent::EncoderTurn(n)) => unsafe {
            if (n + val) != 0 {
                queue.enqueue_unchecked(UIInputEvent::EncoderTurn(n + val))
            }
        },
        Some(other_event) => {
            unsafe { queue.enqueue_unchecked(other_event) };
            queue.enqueue(UIInputEvent::EncoderTurn(val)).unwrap();
        }
        None => {
            unsafe { queue.enqueue_unchecked(UIInputEvent::EncoderTurn(val)) };
        }
    }
}

pub struct Encoder<DT: PinId, CLK: PinId, SW: PinId> {
    driver: RotaryEncoder<Pin<DT, FloatingInput>, Pin<CLK, FloatingInput>>,
    event_queue: Queue<UIInputEvent, 32>,
    _sw: PhantomData<SW>,
}

impl<DT: PinId + BankPinId, CLK: PinId + BankPinId, SW: PinId + BankPinId> Encoder<DT, CLK, SW> {
    pub fn new(
        dt: Pin<DT, FloatingInput>,
        clk: Pin<CLK, FloatingInput>,
        _switch: Pin<SW, FloatingInput>,
    ) -> Self {
        Self {
            driver: RotaryEncoder::new(dt, clk),
            event_queue: Queue::new(),
            _sw: PhantomData,
        }
    }

    pub fn handle_turn(&mut self) {
        self.driver.update();

        let direction = self.driver.direction();

        if direction == Direction::Clockwise {
            update_turns(&mut self.event_queue, 1)
        } else if direction == Direction::Anticlockwise {
            update_turns(&mut self.event_queue, -1)
        }
    }

    pub fn handle_switch(&mut self, state: bool) {
        if state {
            self.event_queue
                .enqueue(UIInputEvent::EncoderSwitch(true))
                .unwrap()
        } else {
            self.event_queue
                .enqueue(UIInputEvent::EncoderSwitch(false))
                .unwrap()
        }
    }

    pub fn iter_messages<'t>(&'t mut self) -> impl Iterator<Item = UIInputEvent> + 't {
        QueuePoppingIter::new(&mut self.event_queue)
    }
}

pub static ROTARY_ENCODER: Mutex<RefCell<Option<Encoder<Gpio21, Gpio22, Gpio0>>>> =
    Mutex::new(RefCell::new(None));

pub fn init_encoder(
    dt: Pin<Gpio21, FloatingInput>,
    clk: Pin<Gpio22, FloatingInput>,
    switch: Pin<Gpio0, FloatingInput>,
) {
    free(|cs| {
        ROTARY_ENCODER
            .borrow(cs)
            .replace(Some(Encoder::new(dt, clk, switch)));
    });
}

fn handle_encoder_interrupt(cs: &CriticalSection) {
    if let Some(ref mut rotary_encoder) = ROTARY_ENCODER.borrow(cs).borrow_mut().deref_mut() {
        rotary_encoder.handle_turn();
    }
}

fn handle_switch_interrupt(cs: &CriticalSection, state: bool) {
    if let Some(ref mut rotary_encoder) = ROTARY_ENCODER.borrow(cs).borrow_mut().deref_mut() {
        rotary_encoder.handle_switch(state);
    }
}

pub fn init_interrupts(pac: &mut Peripherals) {
    // set edge interrupts
    pac.IO_BANK0.proc0_inte[0].modify(|_, w| {
        // GPIO0
        w.gpio0_edge_high().set_bit();
        w.gpio0_edge_low().set_bit()
    });
    pac.IO_BANK0.proc0_inte[2].modify(|_, w| {
        // GPIO22
        w.gpio6_edge_high().set_bit();
        w.gpio6_edge_low().set_bit();
        // GPIO21
        w.gpio5_edge_high().set_bit();
        w.gpio5_edge_low().set_bit()
    });
}

pub fn handle_irq(cs: &CriticalSection, pac: &mut Peripherals) {
    let reg_s = pac.IO_BANK0.proc0_ints[0].read();

    trace!("--- GPIO_IRQ ---");

    if reg_s.gpio0_edge_high().bit() {
        pac.IO_BANK0.intr[0].write(|w| w.gpio0_edge_high().set_bit());
        debounce(cs, pac, 0, 0, DEBOUNCE_INTERVAL, |cs, pac| {
            handle_switch_interrupt(cs, (pac.SIO.gpio_in.read().bits() & 1) == 0);
        });
    } else if reg_s.gpio0_edge_low().bit() {
        pac.IO_BANK0.intr[0].write(|w| w.gpio0_edge_low().set_bit());
        debounce(cs, pac, 0, 0, DEBOUNCE_INTERVAL, |cs, pac| {
            handle_switch_interrupt(cs, (pac.SIO.gpio_in.read().bits() & 1) == 0);
        });
    }

    let reg_s = pac.IO_BANK0.proc0_ints[2].read();

    if reg_s.gpio5_edge_high().bit() {
        handle_encoder_interrupt(cs);
        pac.IO_BANK0.intr[2].write(|w| w.gpio5_edge_high().set_bit());
    }
    if reg_s.gpio5_edge_low().bit() {
        handle_encoder_interrupt(cs);
        pac.IO_BANK0.intr[2].write(|w| w.gpio5_edge_low().set_bit());
    }

    if reg_s.gpio6_edge_high().bit() {
        handle_encoder_interrupt(cs);
        pac.IO_BANK0.intr[2].write(|w| w.gpio6_edge_high().set_bit());
    }
    if reg_s.gpio6_edge_low().bit() {
        handle_encoder_interrupt(cs);
        pac.IO_BANK0.intr[2].write(|w| w.gpio6_edge_low().set_bit());
    }
}
