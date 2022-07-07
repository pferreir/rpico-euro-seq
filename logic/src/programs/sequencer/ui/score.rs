use core::{fmt::Debug, cmp::{max, min}};
use embedded_graphics::{draw_target::DrawTarget, prelude::*, primitives::{Line, Rectangle, PrimitiveStyleBuilder}, pixelcolor::{Rgb555, Rgb565}};
use embedded_sdmmc::{BlockDevice, TimeSource};
use ufmt::uwrite;
use voice_lib::{NotePair, NoteFlag};
use heapless::String;
use crate::util::DiscreetUnwrap;

use crate::{programs::SequencerProgram, screen::SCREEN_WIDTH, log::info};

use super::{NUM_HORIZONTAL_BEATS, NOTE_HEIGHT, NUM_VERTICAL_NOTES, roll::{ROLL_WIDTH, ROLL_HEIGHT, draw_piano_roll}};

const SCORE_WIDTH: u32 = SCREEN_WIDTH as u32 - ROLL_WIDTH as u32;
const PIXELS_PER_BEAT: u32 = SCORE_WIDTH / NUM_HORIZONTAL_BEATS;

impl<'t, B: BlockDevice, TS: TimeSource,  D: DrawTarget<Color = Rgb565>> SequencerProgram<'t, B, TS, D> where <D as DrawTarget>::Error: Debug {
    pub(crate) fn _render_screen(&self, screen: &mut D) {
        let (current_time, beat) = self.state.get_time();
        let start_time =
            current_time as i32 - (NUM_HORIZONTAL_BEATS as i32 / 2 * 60_000 / (self.bpm as i32));
        let start_beat = beat.saturating_sub(NUM_HORIZONTAL_BEATS / 2) as usize;
        screen.clear(Rgb565::CSS_DARK_SLATE_BLUE).unwrap();
        draw_piano_roll(0, self.current_note, screen);
        self.draw_grid(0, start_time, start_beat as u32, screen);

        let mut text = String::<32>::new();
        uwrite!(text, "ITER SINCE {}", start_beat).duwrp();
        info(&text);

        self.draw_notes(
            0,
            self.current_note,
            start_time,
            self.recorder
                .iter_notes_since(start_beat, NUM_HORIZONTAL_BEATS as usize + 1),
            screen,
        );
        self.draw_cursor(0, screen);
        self.draw_buttons(Point::new(10, 100), screen);
    }

    pub(crate) fn draw_grid(&self, top: i32, start_time: i32, start_beat: u32, screen: &mut D) {
        let mark_style = PrimitiveStyleBuilder::new()
            .stroke_color(Rgb565::BLACK)
            .stroke_width(1)
            .build();

        for beat in 0..(NUM_HORIZONTAL_BEATS + 1) {
            let beat_t = (start_beat + beat as u32) * 60_000 / self.bpm as u32;
            let mut x =
                (beat_t as i32 - start_time) as i32 * self.bpm as i32 * PIXELS_PER_BEAT as i32
                    / 60_000;

            if x > 0 {
                x += ROLL_WIDTH as i32 + 1;

                Line::new(Point::new(x, top + 1), Point::new(x, top + ROLL_HEIGHT - 1))
                    .into_styled(mark_style)
                    .draw(screen)
                    .unwrap();
            }
        }
    }

    pub(crate) fn draw_cursor(&self, top: i32, screen: &mut D)
    where
        D: DrawTarget<Color = Rgb565>,
        <D as DrawTarget>::Error: Debug,
    {
        let x = ROLL_WIDTH + 1 + (SCORE_WIDTH as i32 / 2);
        let mark_style = PrimitiveStyleBuilder::new()
            .stroke_color(Rgb565::CSS_RED)
            .stroke_width(1)
            .build();
        Line::new(Point::new(x, top + 1), Point::new(x, top + ROLL_HEIGHT - 1))
            .into_styled(mark_style)
            .draw(screen)
            .unwrap();
    }

    pub(crate) fn draw_notes<I: IntoIterator<Item = (usize, Option<NotePair>, NoteFlag)>>(
        &self,
        top: i32,
        from_note: u8,
        start_time: i32,
        slots: I,
        screen: &mut D,
    ) where
        D: DrawTarget<Color = Rgb565>,
        <D as DrawTarget>::Error: Debug,
    {
        let to_note = from_note.saturating_add(NUM_VERTICAL_NOTES as u8);

        let note_style = PrimitiveStyleBuilder::new()
            .fill_color(Rgb565::BLUE)
            .build();

        for (beat, note, flag) in slots.into_iter() {
            let beat_t = (beat as u32) * 60_000 / self.bpm as u32;

            let start_x =
            max(0, beat_t as i32 - start_time) as u32 * self.bpm as u32 * PIXELS_PER_BEAT
                / 60_000;
            let next_beat_t = beat_t as i32 + 60_000 / self.bpm as i32;
            let end_x = min(
                SCORE_WIDTH - 1,
                (next_beat_t - start_time) as u32 * self.bpm as u32 * PIXELS_PER_BEAT / 60_000,
            );

            match flag {
                NoteFlag::None => {
                    // no note
                    continue;
                },
                NoteFlag::Note | NoteFlag::Legato => {
                    let note: u8 = (&note.unwrap()).into();
                    let mut text = String::<32>::new();
                    uwrite!(text, "{} {}", note, flag as u8).duwrp();
                    info(&text);
                    if (note < from_note) || (note > to_note) {
                        // outside the current view
                        continue;
                    }

                    let y = top + (NUM_VERTICAL_NOTES - 1 - (note - from_note) as i32) * NOTE_HEIGHT;

                    if end_x > start_x {
                        Rectangle::new(
                            Point::new(ROLL_WIDTH as i32 + 1 + (start_x + 1) as i32, y + 1),
                            Size::new(end_x - start_x - 1, NOTE_HEIGHT as u32 - 1),
                        )
                        .into_styled(note_style)
                        .draw(screen)
                        .unwrap();
                    }
                },
            }
        }
    }
}
