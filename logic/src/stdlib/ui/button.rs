use core::{cmp, marker::PhantomData};

use embedded_graphics::{
    draw_target::DrawTarget,
    mono_font::MonoTextStyle,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::{Text, TextStyleBuilder},
    Drawable,
};
use profont::PROFONT_12_POINT;


const MIN_BUTTON_WIDTH: u32 = 30;

pub struct Button<'t, C> {
    text: &'t str,
    selected: bool,
    position: Point,
    _c: PhantomData<C>
}

impl<'t, C: WebColors> Drawable for Button<'t, C> {
    type Color = C;
    type Output = ();

    fn draw<D>(&self, target: &mut D) -> Result<Self::Output, D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        let text_style = MonoTextStyle::new(&PROFONT_12_POINT, C::WHITE);
        let text_style_selected = MonoTextStyle::new(&PROFONT_12_POINT, C::YELLOW);
        let button_style = PrimitiveStyleBuilder::new()
            .fill_color(C::CSS_SLATE_BLUE)
            .stroke_width(1)
            .stroke_color(C::CSS_AQUAMARINE)
            .build();
        let button_style_selected = PrimitiveStyleBuilder::new()
            .fill_color(C::CSS_CORAL)
            .stroke_width(1)
            .stroke_color(C::CSS_CRIMSON)
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

impl<'t, C> Button<'t, C> {
    pub fn new(text: &'t str, position: Point) -> Self {
        Self {
            text,
            selected: false,
            position,
            _c: PhantomData
        }
    }

    pub fn with_selected(self, selected: bool) -> Self {
        Self { selected, ..self }
    }
}
