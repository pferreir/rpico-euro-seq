use core::fmt::Debug;

use embedded_graphics::{
    draw_target::DrawTarget,
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::{Point, Primitive, RgbColor, Size, WebColors},
    primitives::{Line, PrimitiveStyleBuilder, Rectangle},
    text::Text,
    Drawable,
};
use embedded_midi::MidiMessage;
use heapless::{spsc::Queue, String, Vec};

use crate::{
    ui::UIInputEvent,
    util::{midi_note_to_lib, QueuePoppingIter}, gate_cv::Output,
};
use ufmt::uwrite;
use voice_lib::{NotePair, VoiceState};

use super::Program;

const NUM_VOICES: usize = 2;
const HISTORY_SIZE: usize = 256;
const NOTE_HEIGHT: i32 = 6;
const NUM_VERTICAL_NOTES: i32 = 20;
const HEIGHT_ROLL: i32 = NOTE_HEIGHT * NUM_VERTICAL_NOTES;
const MS_PER_WIDTH: u32 = 10000;

pub struct ConverterProgram<'t> {
    current_note: i8,
    voice_state: VoiceState<'t, NUM_VOICES, HISTORY_SIZE>,
    midi_queue: Queue<MidiMessage, 64>,
    current_time: u32,
}

fn draw_timeline<D>(top: i32, from: i8, screen: &mut D)
where
    D: DrawTarget<Color = Rgb565>,
    <D as DrawTarget>::Error: Debug,
{
    let frame_style = PrimitiveStyleBuilder::new()
        .stroke_color(Rgb565::WHITE)
        .stroke_width(1)
        .fill_color(Rgb565::BLACK)
        .build();

    let rect = Rectangle::new(Point::new(0, top), Size::new(240, HEIGHT_ROLL as u32));
    rect.into_styled(frame_style).draw(screen).unwrap();

    let mark_style = PrimitiveStyleBuilder::new()
        .stroke_color(Rgb565::WHITE)
        .stroke_width(1)
        .build();
    let white_key_style = PrimitiveStyleBuilder::new()
        .fill_color(Rgb565::WHITE)
        .build();
    let line_style = PrimitiveStyleBuilder::new()
        .stroke_color(Rgb565::CSS_GRAY)
        .stroke_width(1)
        .build();

    for i in 0..20 {
        Line::new(
            Point::new(0, top + (NUM_VERTICAL_NOTES - 1 - i) * NOTE_HEIGHT),
            Point::new(6, top + (NUM_VERTICAL_NOTES - 1 - i) * NOTE_HEIGHT),
        )
        .into_styled(mark_style)
        .draw(screen)
        .unwrap();
        Line::new(
            Point::new(7, top + (NUM_VERTICAL_NOTES - 1 - i) * NOTE_HEIGHT),
            Point::new(238, top + (NUM_VERTICAL_NOTES - 1 - i) * NOTE_HEIGHT),
        )
        .into_styled(line_style)
        .draw(screen)
        .unwrap();

        let NotePair(note, _) = (from + i as i8).into();
        if !note.is_black_key() {
            Rectangle::new(
                Point::new(1, top + (NUM_VERTICAL_NOTES - 1 - i) * NOTE_HEIGHT),
                Size::new(5, NOTE_HEIGHT as u32),
            )
            .into_styled(white_key_style)
            .draw(screen)
            .unwrap();
        }
    }

    Line::new(Point::new(6, top), Point::new(6, top + HEIGHT_ROLL - 1))
        .into_styled(mark_style)
        .draw(screen)
        .unwrap();
}

fn draw_notes<'t, D, I: IntoIterator<Item = &'t (NotePair, u32, Option<u32>)>>(
    top: i32,
    from_note: i8,
    start_time: u32,
    slots: I,
    screen: &mut D,
) where
    D: DrawTarget<Color = Rgb565>,
    <D as DrawTarget>::Error: Debug,
{
    let to_note = from_note.saturating_add(NUM_VERTICAL_NOTES as i8);

    let note_style = PrimitiveStyleBuilder::new()
        .fill_color(Rgb565::BLUE)
        .build();

    for (note, start, end) in slots {
        let note: i8 = note.into();
        if (note < from_note) || (note > to_note) {
            continue;
        }
        let start_x = if *start <= start_time {
            0
        } else {
            239 * (start - start_time) / MS_PER_WIDTH
        };
        let end_x = match end {
            Some(end_time) => 239 * (end_time - start_time) / MS_PER_WIDTH,
            None => 239,
        };
        let y = top + (NUM_VERTICAL_NOTES - 1 - (note - from_note) as i32) * NOTE_HEIGHT;

        if end_x > start_x {
            Rectangle::new(
                Point::new(start_x as i32, y),
                Size::new(end_x - start_x, NOTE_HEIGHT as u32),
            )
            .into_styled(note_style)
            .draw(screen)
            .unwrap();
        }
    }
}

impl<'t> Program for ConverterProgram<'t> {
    fn new() -> Self {
        Self {
            current_note: 72, // C5,
            voice_state: VoiceState::new(),
            midi_queue: Queue::new(),
            current_time: 0,
        }
    }

    fn render_screen<D>(&self, screen: &mut D)
    where
        D: DrawTarget<Color = Rgb565>,
        <D as DrawTarget>::Error: Debug,
    {
        screen.clear(Rgb565::CSS_DARK_SLATE_BLUE).unwrap();
        draw_timeline(0, self.current_note, screen);
        draw_notes(
            0,
            self.current_note,
            self.current_time.saturating_sub(MS_PER_WIDTH),
            self.voice_state.since(self.current_time.saturating_sub(MS_PER_WIDTH)),
            screen,
        );

        let STYLE_CYAN: MonoTextStyle<Rgb565> = MonoTextStyle::new(&FONT_10X20, Rgb565::CYAN);

        for n_voice in 0..NUM_VOICES {
            let mut txt = String::<16>::new();
            uwrite!(txt, "V{}: ", n_voice).unwrap();
            match self.voice_state[n_voice as u8] {
                Some(np) => {
                    uwrite!(txt, "{}", np).unwrap();
                }
                None => uwrite!(txt, "-").unwrap(),
            };
            Text::new(&txt, Point::new(10, 140 + n_voice as i32 * 15), STYLE_CYAN)
                .draw(screen)
                .unwrap();
        }
    }

    fn process_ui_input(&mut self, msg: &UIInputEvent) {
        match msg {
            UIInputEvent::EncoderTurn(v) => {
                let new_current_note = (self.current_note as i8) + v;
                self.current_note = if new_current_note < 0 {
                    0
                } else {
                    new_current_note as i8
                }
            }
            _ => {}
        }
    }

    fn process_midi(&mut self, msg: &MidiMessage) {
        self.midi_queue.enqueue(msg.clone()).unwrap();
    }

    fn update_output(&self, output: &mut impl Output) {
        let voices: Vec<&Option<NotePair>, NUM_VOICES> = self.voice_state.iter_voices().collect();

        match voices[0] {
            Some(n) => {
                output.set_ch0(n.into());
                output.set_gate0(true);
            },
            None => {
                output.set_gate0(false);
            }
        }

        match voices[1] {
            Some(n) => {
                output.set_ch1(n.into());
                output.set_gate1(true);
            },
            None => {
                output.set_gate1(false);
            }
        }
    }

    fn run(&mut self, program_time: u32) {
        self.current_time = program_time;
        for msg in QueuePoppingIter::new(&mut self.midi_queue) {
            match msg {
                MidiMessage::NoteOff(_, n, _) => {
                    self.voice_state.clear(midi_note_to_lib(n), program_time);
                }
                MidiMessage::NoteOn(_, n, v) => {
                    if v == 0.into() {
                        // equivalent to NoteOff
                        self.voice_state.clear(midi_note_to_lib(n), program_time);
                    } else {
                        self.voice_state.set(midi_note_to_lib(n), program_time);
                    }
                }
                _ => {}
            }
        }
    }
}
