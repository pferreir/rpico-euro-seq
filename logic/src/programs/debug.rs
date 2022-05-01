use core::fmt::Debug;
use embedded_graphics::{
    draw_target::DrawTarget,
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, PrimitiveStyleBuilder},
    text::Text,
    Drawable,
};
use embedded_midi::MidiMessage;
use heapless::{spsc::Queue, String};
use ufmt::uwrite;

use crate::ui::UIInputEvent;

use super::{Program};

extern "C" {
    static _stack_start: u32;
}

pub struct DebugProgram {
    messages: Queue<MidiMessage, 5>,
    fps: u8,
    encoder_pos: i8,
    encoder_sw_state: bool,
    sw1_state: bool,
    sw2_state: bool,
    mem_usage: u32,
    last_tick: u32,
    frame_counter: u8,
}

impl Program for DebugProgram {
    fn new() -> Self {
        Self {
            messages: Queue::new(),
            mem_usage: 0,
            fps: 0,
            encoder_pos: 0,
            encoder_sw_state: false,
            sw1_state: false,
            sw2_state: false,
            last_tick: 0,
            frame_counter: 0,
        }
    }
    fn render_screen<D>(&self, screen: &mut D)
    where
        D: DrawTarget<Color = Rgb565>,
        <D as DrawTarget>::Error: Debug,
    {
        let STYLE_YELLOW: MonoTextStyle<Rgb565> = MonoTextStyle::new(&FONT_10X20, Rgb565::YELLOW);
        let STYLE_RED: MonoTextStyle<Rgb565> = MonoTextStyle::new(&FONT_10X20, Rgb565::RED);
        let STYLE_CYAN: MonoTextStyle<Rgb565> = MonoTextStyle::new(&FONT_10X20, Rgb565::CYAN);

        let STYLE_FILLED = PrimitiveStyleBuilder::new()
            .stroke_color(Rgb565::WHITE)
            .stroke_width(1)
            .build();
        let STYLE_EMPTY = PrimitiveStyleBuilder::new()
            .fill_color(Rgb565::WHITE)
            .build();

        let mut out = String::<128>::new();

        uwrite!(
            out,
            "=> {} | {}",
            self.encoder_pos,
            if self.encoder_sw_state { "ON" } else { "OFF" }
        )
        .unwrap();

        Circle::new(Point::new(20, 30), 20)
            .into_styled(if self.sw1_state {
                STYLE_FILLED
            } else {
                STYLE_EMPTY
            })
            .draw(screen)
            .unwrap();

            Circle::new(Point::new(60, 30), 20)
            .into_styled(if self.sw2_state {
                STYLE_FILLED
            } else {
                STYLE_EMPTY
            })
            .draw(screen)
            .unwrap();

        Text::new(&out, Point::new(20, 15), STYLE_CYAN)
            .draw(screen)
            .unwrap();

        out.truncate(0);
        for msg in self.messages.iter() {
            match msg {
                embedded_midi::MidiMessage::NoteOff(_, _, _) => uwrite!(out, "OFF"),
                embedded_midi::MidiMessage::NoteOn(chan, note, vel) => uwrite!(
                    out,
                    "N-{}-{}-{}",
                    Into::<u8>::into(*chan),
                    Into::<u8>::into(*note),
                    Into::<u8>::into(*vel)
                ),
                _ => uwrite!(out, "Whatever"),
            }
            .unwrap();
            uwrite!(out, "\n").unwrap();
        }

        Text::new(&out, Point::new(20, 160), STYLE_YELLOW)
            .draw(screen)
            .unwrap();

        out.truncate(0);
        uwrite!(out, "{} fps", self.fps).unwrap();

        Text::new(&out, Point::new(20, 100), STYLE_RED)
            .draw(screen)
            .unwrap();

        out.truncate(0);
        uwrite!(out, "{}KB", self.mem_usage / 1024).unwrap();
        Text::new(&out, Point::new(180, 220), STYLE_CYAN)
            .draw(screen)
            .unwrap();
    }

    fn process_midi(&mut self, msg: &MidiMessage) {
        match self.messages.enqueue(msg.clone()) {
            Ok(()) => {}
            Err(rej_msg) => {
                self.messages.dequeue();
                unsafe { self.messages.enqueue_unchecked(rej_msg) };
            }
        }
    }

    fn process_ui_input(&mut self, msg: &UIInputEvent) {
        match msg {
            UIInputEvent::EncoderTurn(n) => {
                self.encoder_pos += n;
            }
            UIInputEvent::EncoderSwitch(v) => {
                self.encoder_sw_state = *v;
            }
            UIInputEvent::Switch1(v) => {
                self.sw1_state = *v;
            }
            UIInputEvent::Switch2(v) => {
                self.sw2_state = *v;
            }
        }
    }

    fn run(&mut self, program_time: u32) {
        let diff = program_time - self.last_tick;
        if diff >= 1_000u32 {
            self.fps = (self.frame_counter as u32 * 1_000 / diff) as u8;
            self.frame_counter = 0;
            self.last_tick = program_time;
        }
        self.frame_counter = self.frame_counter.wrapping_add(1);

        // stack grows from end of RAM
        // = 0x20000000 + 256k
        // let sp = unsafe { &_stack_start as *const u32 as u32 };
        // self.mem_usage = sp - cortex_m::register::msp::read();
        self.mem_usage = 0;
    }
}
