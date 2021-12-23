use core::fmt::Debug;

use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::Rgb565,
    prelude::{Point, Primitive, RgbColor, Size, WebColors},
    primitives::{Line, PrimitiveStyleBuilder, Rectangle},
    Drawable,
};
use embedded_midi::MidiMessage;

use super::Program;

pub struct ConverterProgram {}

const NOTE_HEIGHT: i32 = 6;
const NUM_VERTICAL_NOTES: i32 = 20;
const HEIGHT_ROLL:i32 = NOTE_HEIGHT * NUM_VERTICAL_NOTES;

enum Note {
    C,
    Db,
    D,
    Eb,
    E,
    F,
    Gb,
    G,
    Ab,
    A,
    Bb,
    B,
}

impl Note {
    pub fn is_black_key(&self) -> bool {
        match self {
            Note::Db | Note::Eb | Note::Gb | Note::Ab | Note::Bb => true,
            _ => false,
        }
    }
}

fn midi_to_note(midi: u8) -> (Note, i8) {
    let note = match (midi - 21) % 12 {
        0 => Note::C,
        1 => Note::Db,
        2 => Note::D,
        3 => Note::Eb,
        4 => Note::E,
        5 => Note::F,
        6 => Note::Gb,
        7 => Note::G,
        8 => Note::Ab,
        9 => Note::A,
        10 => Note::Bb,
        11 => Note::B,
        _ => unreachable!(),
    };
    (note, (midi as i8 - 21) / 12)
}

fn draw_timeline<D>(top: i32, from: u8, screen: &mut D)
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

        let (note, _) = midi_to_note(from + ((NUM_VERTICAL_NOTES - 1 - i) as u8));
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

impl Program for ConverterProgram {
    fn new() -> Self {
        Self {}
    }

    fn render_screen<D>(&self, screen: &mut D)
    where
        D: DrawTarget<Color = Rgb565>,
        <D as DrawTarget>::Error: Debug,
    {
        screen.clear(Rgb565::CSS_DARK_SLATE_BLUE).unwrap();
        draw_timeline(0, 0, screen);
    }

    fn process_midi(&mut self, msg: &MidiMessage) {}

    fn run(&mut self, program_time: u64) {}
}
