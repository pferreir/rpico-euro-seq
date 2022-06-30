use core::{cmp, marker::PhantomData};

use embedded_graphics::{
    draw_target::DrawTarget,
    mono_font::MonoTextStyle,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::{Text, TextStyleBuilder},
    Drawable, pixelcolor::Rgb565,
};
use profont::PROFONT_12_POINT;

use super::{select::{Selectable}, DynDrawable};


const MIN_BUTTON_WIDTH: u32 = 30;

pub struct Button {
    text: &'static str,
    selected: bool,
    position: Point
}

impl<T: DrawTarget<Color = Rgb565>> DynDrawable<T> for Button {
    fn draw(&self, target: &mut T) -> Result<(), T::Error>
    where
        T: DrawTarget<Color = Rgb565>,
    {
        let text_style = MonoTextStyle::new(&PROFONT_12_POINT, Rgb565::WHITE);
        let text_style_selected = MonoTextStyle::new(&PROFONT_12_POINT, Rgb565::YELLOW);
        let button_style = PrimitiveStyleBuilder::new()
            .fill_color(Rgb565::CSS_SLATE_BLUE)
            .stroke_width(1)
            .stroke_color(Rgb565::CSS_AQUAMARINE)
            .build();
        let button_style_selected = PrimitiveStyleBuilder::new()
            .fill_color(Rgb565::CSS_CORAL)
            .stroke_width(1)
            .stroke_color(Rgb565::CSS_CRIMSON)
            .build();

        let mut text = Text::with_text_style(
            self.text,
            self.position,
            if self.selected {
                text_style
            } else {
                text_style_selected
            },
            TextStyleBuilder::new()
                .alignment(embedded_graphics::text::Alignment::Center)
                .baseline(embedded_graphics::text::Baseline::Middle)
                .build(),
        );

        let Rectangle { size, .. } = text.bounding_box();
        let size = Size::new(cmp::max(size.width, MIN_BUTTON_WIDTH), size.height);
        let padding = Size::new(10, 5);

        Rectangle::new(self.position, size + padding)
            .into_styled(if self.selected {
                button_style_selected
            } else {
                button_style
            })
            .draw(target)?;

        text.position += size / 2 + padding / 2;
        text.draw(target)?;

        Ok(())
    }
}

impl Button {
    pub fn new(text: &'static str, position: Point) -> Self {
        Self {
            text,
            selected: false,
            position
        }
    }
}

impl<T: DrawTarget<Color = Rgb565>> Selectable<T> for Button {
    fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}
