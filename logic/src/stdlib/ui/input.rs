use core::marker::PhantomData;

use embedded_graphics::{
    mono_font::MonoTextStyle,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::Text,
    Drawable,
};
use heapless::String;
use profont::PROFONT_12_POINT;

use super::select::Selectable;

pub struct Input<'t, C> {
    text: &'t str,
    position: Point,
    selected: bool,
    length: u8,
    _c: PhantomData<C>,
}

impl<'t, C: WebColors> Drawable for Input<'t, C> {
    type Color = C;
    type Output = ();

    fn draw<D>(&self, target: &mut D) -> Result<Self::Output, D::Error>
    where
        D: embedded_graphics::draw_target::DrawTarget<Color = Self::Color>,
    {
        let text_style = MonoTextStyle::new(&PROFONT_12_POINT, C::WHITE);
        let input_style = PrimitiveStyleBuilder::new()
            .fill_color(C::CSS_DIM_GRAY)
            .stroke_width(1)
            .stroke_color(C::CSS_AQUAMARINE)
            .build();
        let input_style_selected = PrimitiveStyleBuilder::new()
            .fill_color(C::CSS_DIM_GRAY)
            .stroke_width(1)
            .stroke_color(C::CSS_YELLOW)
            .build();

        let string = String::<64>::from(
            &"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
                [..self.length as usize],
        );
        let mut text = Text::with_baseline(
            &string,
            Point::new(0, 0),
            text_style,
            embedded_graphics::text::Baseline::Bottom,
        );

        let Rectangle { size, .. } = text.bounding_box();
        let padding = Size::new(10, 5);

        Rectangle::new(self.position, size + padding)
            .into_styled(if self.selected {
                input_style_selected
            } else {
                input_style
            })
            .draw(target)?;

        text.text = &self.text;
        text.position = self.position + Size::new(0, size.height) + padding / 2;
        text.draw(target)?;

        Ok(())
    }
}

impl<'t, C> Input<'t, C> {
    pub fn new(text: &'t str, position: Point) -> Self {
        Self {
            text,
            position,
            selected: false,
            length: 12,
            _c: PhantomData,
        }
    }

    pub fn with_length(self, length: u8) -> Self {
        Self { length, ..self }
    }

    pub fn with_selected(self, selected: bool) -> Self {
        Self { selected, ..self }
    }
}

impl<'t, C: WebColors> Selectable for Input<'t, C> {
    fn with_selected(self, selected: bool) -> Self {
        Self { selected, ..self }
    }
}
