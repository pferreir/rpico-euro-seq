use core::fmt::Debug;
use embedded_graphics::{
    draw_target::DrawTarget, mono_font::{MonoTextStyle, ascii::FONT_10X20}, prelude::*,
    text::Text, pixelcolor::Rgb565, Drawable,
};
use embedded_midi::MidiMessage;
use heapless::{spsc::Queue, String};
use ufmt::uwrite;

use super::Program;
pub struct DebugProgram {
    messages: Queue<MidiMessage, 5>,
    fps: u8,
    last_tick: u64,
    frame_counter: u8
}

impl Program for DebugProgram {
    fn new() -> Self {
        Self {
            messages: Queue::new(),
            fps: 0,
            last_tick: 0,
            frame_counter: 0
        }
    }
    fn render_screen<D>(&self, screen: &mut D) where
        D: DrawTarget<Color=Rgb565>,
        <D as DrawTarget>::Error: Debug
    {
        let style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
        let mut out = String::<128>::new();
        for msg in self.messages.iter() {
            match msg {
                embedded_midi::MidiMessage::NoteOff(_, _, _) => uwrite!(out, "NoteOff"),
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

        Text::new(&out, Point::new(20, 160), style)
            .draw(screen)
            .unwrap();

        out.truncate(0);
        uwrite!(out, "{} fps", self.fps).unwrap();

        Text::new(&out, Point::new(20, 100), style)
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

    fn run(&mut self, program_time: u64) {
        let diff = (program_time - self.last_tick) as u32;
        if diff >= 1_000_000u32 {
            self.fps = (self.frame_counter as u32 * 1_000_000 / diff) as u8;
            self.frame_counter = 0;
            self.last_tick = program_time;
        }
        self.frame_counter += 1;
    }
}
