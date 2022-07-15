use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle, Line},
};
use voice_lib::NotePair;

use crate::{screen::SCREEN_WIDTH, util::DiscreetUnwrap};

use super::{NUM_VERTICAL_NOTES, NOTE_HEIGHT};

pub(crate) const ROLL_WIDTH: i32 = 6;
pub(crate) const ROLL_HEIGHT: i32 = NOTE_HEIGHT * NUM_VERTICAL_NOTES;

pub(crate) fn draw_piano_roll<D>(top: i32, from: u8, screen: &mut D)
where
    D: DrawTarget<Color = Rgb565>
{
    let frame_style = PrimitiveStyleBuilder::new()
        .stroke_color(Rgb565::WHITE)
        .stroke_width(1)
        .fill_color(Rgb565::BLACK)
        .build();

    let rect = Rectangle::new(
        Point::new(0, 0),
        Size::new(SCREEN_WIDTH as u32, ROLL_HEIGHT as u32 + 1),
    );
    rect.into_styled(frame_style).draw(screen).duwrp();

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
        .duwrp();

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
        .duwrp();

        let NotePair(note, _) = (from + i as u8).into();
        if !note.is_black_key() {
            Rectangle::new(
                Point::new(1, top + (NUM_VERTICAL_NOTES - 1 - i) * NOTE_HEIGHT),
                Size::new(5, NOTE_HEIGHT as u32),
            )
            .into_styled(white_key_style)
            .draw(screen)
            .duwrp();
        }
    }

    Line::new(
        Point::new(ROLL_WIDTH, top),
        Point::new(ROLL_WIDTH, top + ROLL_HEIGHT - 1),
    )
    .into_styled(mark_style)
    .draw(screen)
    .duwrp();
}
