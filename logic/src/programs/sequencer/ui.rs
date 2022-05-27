use core::{
    cmp::{max, min},
    fmt::Debug,
};

use embedded_graphics::{
    draw_target::DrawTarget,
    image::Image,
    pixelcolor::Rgb565,
    prelude::{Point, Primitive, RgbColor, Size, WebColors},
    primitives::PrimitiveStyle,
    primitives::{Line, PrimitiveStyleBuilder, Rectangle},
    Drawable,
};
use embedded_sdmmc::{TimeSource, BlockDevice};
use voice_lib::{NoteFlag, NotePair};

use super::{State, SequencerProgram};

const SCREEN_WIDTH: u32 = crate::screen::SCREEN_WIDTH as u32;

const NOTE_HEIGHT: i32 = 4;
const NUM_VERTICAL_NOTES: i32 = 20;
const NUM_HORIZONTAL_BEATS: u32 = 22;
const ROLL_WIDTH: i32 = 6;
const ROLL_HEIGHT: i32 = NOTE_HEIGHT * NUM_VERTICAL_NOTES;
const SCORE_WIDTH: u32 = SCREEN_WIDTH - ROLL_WIDTH as u32;
const PIXELS_PER_BEAT: u32 = SCORE_WIDTH / NUM_HORIZONTAL_BEATS;

pub(crate) const NUM_UI_ACTIONS: usize = 5;

pub(crate) mod icons {
    pub(crate) static PLAY_ICON: &[u8] = include_bytes!("../../../assets/play.bmp");
    pub(crate) static PAUSE_ICON: &[u8] = include_bytes!("../../../assets/pause.bmp");
    pub(crate) static RECORD_ICON: &[u8] = include_bytes!("../../../assets/record.bmp");
    pub(crate) static RECORD_ON_ICON: &[u8] = include_bytes!("../../../assets/record_on.bmp");
    pub(crate) static STOP_ICON: &[u8] = include_bytes!("../../../assets/stop.bmp");
    pub(crate) static STOP_ON_ICON: &[u8] = include_bytes!("../../../assets/stop_on.bmp");
    pub(crate) static BEGINNING_ICON: &[u8] = include_bytes!("../../../assets/beginning.bmp");
    pub(crate) static SEEK_ICON: &[u8] = include_bytes!("../../../assets/seek.bmp");
}

#[derive(Copy, Clone)]
#[repr(u8)]
pub(crate) enum UIAction {
    PlayPause = 0,
    Stop = 1,
    Record = 2,
    Beginning = 3,
    Seek = 4,
}

impl UIAction {
    fn button_pos(&self) -> Point {
        match self {
            UIAction::PlayPause => Point::new(0, 0),
            UIAction::Stop => Point::new(25, 0),
            UIAction::Record => Point::new(50, 0),
            UIAction::Beginning => Point::new(80, 0),
            UIAction::Seek => Point::new(105, 0),
        }
    }
}

impl From<u8> for UIAction {
    fn from(v: u8) -> Self {
        match v % NUM_UI_ACTIONS as u8 {
            0 => UIAction::PlayPause,
            1 => UIAction::Stop,
            2 => UIAction::Record,
            3 => UIAction::Beginning,
            4 => UIAction::Seek,
            _ => unreachable!(),
        }
    }
}

pub(crate) fn draw_timeline<D>(top: i32, from: u8, screen: &mut D)
where
    D: DrawTarget<Color = Rgb565>,
    <D as DrawTarget>::Error: Debug,
{
    let frame_style = PrimitiveStyleBuilder::new()
        .stroke_color(Rgb565::WHITE)
        .stroke_width(1)
        .fill_color(Rgb565::BLACK)
        .build();

    let rect = Rectangle::new(
        Point::new(0, 0),
        Size::new(SCREEN_WIDTH, ROLL_HEIGHT as u32 + 1),
    );
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

    // lines
    for i in 0..NUM_VERTICAL_NOTES {
        Line::new(
            Point::new(0, top + (NUM_VERTICAL_NOTES - 1 - i) * NOTE_HEIGHT),
            Point::new(ROLL_WIDTH, top + (NUM_VERTICAL_NOTES - 1 - i) * NOTE_HEIGHT),
        )
        .into_styled(mark_style)
        .draw(screen)
        .unwrap();
        Line::new(
            Point::new(
                ROLL_WIDTH + 1,
                top + (NUM_VERTICAL_NOTES - 1 - i) * NOTE_HEIGHT,
            ),
            Point::new(
                (SCREEN_WIDTH - 2) as i32,
                top + (NUM_VERTICAL_NOTES - 1 - i) * NOTE_HEIGHT,
            ),
        )
        .into_styled(line_style)
        .draw(screen)
        .unwrap();

        let NotePair(note, _) = (from + i as u8).into();
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

    Line::new(
        Point::new(ROLL_WIDTH, top),
        Point::new(ROLL_WIDTH, top + ROLL_HEIGHT - 1),
    )
    .into_styled(mark_style)
    .draw(screen)
    .unwrap();
}

impl<'t, B: BlockDevice, TS: TimeSource> SequencerProgram<'t, B, TS> {
    pub(crate) fn _render_screen<D>(&self, screen: &mut D)
    where
        D: DrawTarget<Color = Rgb565>,
        <D as DrawTarget>::Error: Debug,
    {
        let (current_time, beat) = self.state.get_time();
        let start_time =
            current_time as i32 - (NUM_HORIZONTAL_BEATS as i32 / 2 * 60_000 / (self.bpm as i32));
        let start_beat = beat.saturating_sub(NUM_HORIZONTAL_BEATS / 2) as usize;
        screen.clear(Rgb565::CSS_DARK_SLATE_BLUE).unwrap();
        draw_timeline(0, self.current_note, screen);
        self.draw_grid(0, start_time, start_beat as u32, screen);

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

    pub(crate) fn draw_buttons<D>(&self, pos: Point, screen: &mut D)
    where
        D: DrawTarget<Color = Rgb565>,
        <D as DrawTarget>::Error: Debug,
    {
        Image::new(
            if let State::Playing(_, _) = self.state {
                &self.pause_icon
            } else {
                &self.play_icon
            },
            pos + UIAction::PlayPause.button_pos(),
        )
        .draw(screen)
        .unwrap();
        Image::new(
            if let State::Stopped = self.state {
                &self.stop_on_icon
            } else {
                &self.stop_icon
            },
            pos + UIAction::Stop.button_pos(),
        )
        .draw(screen)
        .unwrap();
        Image::new(
            if let State::Recording(_, _) = self.state {
                &self.record_on_icon
            } else {
                &self.record_icon
            },
            pos + UIAction::Record.button_pos(),
        )
        .draw(screen)
        .unwrap();
        Image::new(&self.beginning_icon, pos + UIAction::Beginning.button_pos())
            .draw(screen)
            .unwrap();
        Image::new(&self.seek_icon, pos + UIAction::Seek.button_pos())
            .draw(screen)
            .unwrap();

        Rectangle::new(pos + self.selected_action.button_pos(), Size::new(26, 16))
            .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 1))
            .draw(screen)
            .unwrap();
    }

    pub(crate) fn draw_notes<D, I: IntoIterator<Item = (usize, NotePair, NoteFlag)>>(
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
            let note: u8 = (&note).into();
            if (note < from_note) || (note > to_note) {
                continue;
            }
            let beat_t = (beat as u32) * 60_000 / self.bpm as u32;
            let start_x =
                max(0, beat_t as i32 - start_time) as u32 * self.bpm as u32 * PIXELS_PER_BEAT
                    / 60_000;
            let next_beat_t = beat_t as i32 + 60_000 / self.bpm as i32;
            let end_x = min(
                SCORE_WIDTH - 1,
                (next_beat_t - start_time) as u32 * self.bpm as u32 * PIXELS_PER_BEAT / 60_000,
            );

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
        }
    }

    pub(crate) fn draw_grid<D>(&self, top: i32, start_time: i32, start_beat: u32, screen: &mut D)
    where
        D: DrawTarget<Color = Rgb565>,
        <D as DrawTarget>::Error: Debug,
    {
        let mark_style = PrimitiveStyleBuilder::new()
            .stroke_color(Rgb565::new(12, 24, 9))
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

    pub(crate) fn draw_cursor<D>(&self, top: i32, screen: &mut D)
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
}
