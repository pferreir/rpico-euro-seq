use core::marker::PhantomData;

use embedded_graphics::{
    mono_font::MonoTextStyle,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::Text,
    Drawable, pixelcolor::Rgb565,
};
use heapless::String;
use profont::PROFONT_12_POINT;

use crate::ui::UIInputEvent;

use super::{select::{Selectable, Message}, DynDrawable};

pub struct Input {
    text: &'static str,
    position: Point,
    selected: bool,
    editing: bool
}

impl<T: DrawTarget<Color = Rgb565>> DynDrawable<T> for Input {

    fn draw(&self, target: &mut T) -> Result<(), T::Error> {
        let text_style = MonoTextStyle::new(&PROFONT_12_POINT, Rgb565::WHITE);
        let input_style = PrimitiveStyleBuilder::new()
            .fill_color(Rgb565::CSS_DIM_GRAY)
            .stroke_width(1)
            .stroke_color(Rgb565::CSS_AQUAMARINE)
            .build();
        let input_style_selected = PrimitiveStyleBuilder::new()
            .fill_color(Rgb565::CSS_DIM_GRAY)
            .stroke_width(1)
            .stroke_color(Rgb565::CSS_YELLOW)
            .build();

        let string = String::<64>::from(
            &"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
                [..12],
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

impl Input {
    pub fn new(text: &'static str, position: Point) -> Self {
        Self {
            text,
            selected: false,
            editing: false,
            position
        }
    }
}

impl<T: DrawTarget<Color = Rgb565>> Selectable<T> for Input {
    fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
        self.editing = false;
    }

    fn is_selected(&self) -> bool {
        self.selected
    }

    fn process_ui_input(
        &mut self,
        event: &UIInputEvent,
    ) -> Message {
        match event {
            UIInputEvent::EncoderTurn(_) => {
                // TODO: actual editing of input
                Message::None
            },
            UIInputEvent::EncoderSwitch(true) => {
                if self.editing {
                    self.editing = false;
                    Message::StrInput(self.text)
                } else {
                    self.editing = true;
                    Message::None
                }
            },
            _ => { Message::None }
        }
    }
}
