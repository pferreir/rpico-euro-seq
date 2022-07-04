use core::{any::Any, marker::PhantomData};

use crate::{ui::UIInputEvent, log::info};
use alloc::{boxed::Box, vec::Vec};
use embedded_graphics::{
    draw_target::DrawTarget, pixelcolor::Rgb565, prelude::WebColors, Drawable,
};

use super::{button::ButtonId, Button, DynDrawable, Input, OverlayResult};

trait Settings: Default {}
trait Config: Default {}

pub enum Message<'t> {
    None,
    StrInput(&'t str),
    ButtonPress(Box<dyn ButtonId>),
}

impl<'t> PartialEq for Message<'t> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Message::None, Message::None) => true,
            (Message::StrInput(s1), Message::StrInput(s2)) => s1 == s2,
            (Message::ButtonPress(bid1), Message::ButtonPress(bid2)) => {
                ButtonId::eq(bid1.as_ref(), bid2.as_ref())
            }
            _ => false,
        }
    }
}

pub(crate) trait Selectable<D: DrawTarget<Color = Rgb565>>: DynDrawable<D> {
    fn is_selected(&self) -> bool;
    fn set_selected(&mut self, state: bool);
    fn process_ui_input(&mut self, input: &UIInputEvent) -> Message;
}

pub(crate) struct SelectGroup<D: DrawTarget<Color = Rgb565>> {
    counter: usize,
    delegating: bool,
    elements: Vec<Box<dyn Selectable<D>>>,
    selected: usize,
}

impl<D: DrawTarget<Color = Rgb565>> SelectGroup<D> {
    pub fn new() -> Self {
        Self {
            counter: 0,
            delegating: false,
            elements: Vec::new(),
            selected: 0,
        }
    }

    pub fn add<S: Selectable<D> + 'static>(&mut self, elem: S) {
        self.elements.push(Box::new(elem));

        if self.elements.len() == 1 {
            self.elements[0].set_selected(true);
        }

        self.counter += 1;
    }

    pub fn change(&mut self, v: i8) {
        self.selected = (self.selected as i16 + v as i16).rem_euclid(self.counter as i16) as usize;

        for (n, e) in self.elements.iter_mut().enumerate() {
            e.set_selected(n == self.selected as usize);
        }
    }

    pub fn process_ui_input(&mut self, input: &UIInputEvent) -> Message {
        if self.delegating {
            let msg = self.elements[self.selected].process_ui_input(input);
            match msg {
                Message::None => {},
                Message::StrInput(_) => {
                    self.delegating = false;
                },
                Message::ButtonPress(_) => {
                    self.delegating = false;
                },
            }
            msg
        } else {
            match input {
                UIInputEvent::EncoderTurn(v) => {
                    self.change(*v);
                    Message::None
                }
                UIInputEvent::EncoderSwitch(true) => {
                    self.delegating = true;
                    // process event at delegated widget too
                    self.process_ui_input(input)
                }
                _ => Message::None
            }
        }
    }
}

impl<D: DrawTarget<Color = Rgb565>> DynDrawable<D> for SelectGroup<D> {
    fn draw(&self, target: &mut D) -> Result<(), D::Error> {
        for elem in self.elements.iter() {
            elem.draw(target)?;
        }
        Ok(())
    }
}
