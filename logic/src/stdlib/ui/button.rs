use core::{cmp, marker::PhantomData, any::Any};

use alloc::boxed::Box;
use embedded_graphics::{
    draw_target::DrawTarget,
    mono_font::MonoTextStyle,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::{Text, TextStyleBuilder},
    Drawable, pixelcolor::Rgb565,
};
use heapless::String;
use profont::PROFONT_12_POINT;
use ufmt::uwrite;

use crate::{ui::UIInputEvent, log::info};

use super::{select::{Selectable, Message}, DynDrawable};


const MIN_BUTTON_WIDTH: u32 = 30;

trait ComparableButtonId: PartialEq {}

pub trait ButtonId {
    fn clone(&self) -> Box<dyn ButtonId>;
    fn eq(&self, other: &dyn ButtonId) -> bool;
    fn as_any(&self) -> &dyn Any;
}

pub struct Button<ID: ButtonId> {
    text: &'static str,
    selected: bool,
    position: Point,
    id: ID
}

impl<T: DrawTarget<Color = Rgb565>, ID: ButtonId> DynDrawable<T> for Button<ID> {
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

impl<ID: ButtonId> Button<ID> {
    pub fn new(id: ID, text: &'static str, position: Point) -> Self {
        Self {
            text,
            selected: false,
            position,
            id
        }
    }
}

impl<T: DrawTarget<Color = Rgb565>, ID: ButtonId> Selectable<T> for Button<ID> where
    ID: 'static
{
    fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }

    fn is_selected(&self) -> bool {
        self.selected
    }

    fn process_ui_input(
        &mut self,
        input: &UIInputEvent,
    ) -> Message {
        match input {
            UIInputEvent::EncoderSwitch(true) => Message::ButtonPress(self.id.clone()),
            _ => Message::None
        }
    }
}
