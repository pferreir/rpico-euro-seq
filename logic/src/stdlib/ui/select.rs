use core::{marker::PhantomData, any::Any};

use alloc::{boxed::Box, vec::Vec};
use embedded_graphics::{draw_target::DrawTarget, prelude::WebColors, Drawable, pixelcolor::Rgb565};
use super::{Input, Button, DynDrawable};

trait Settings: Default {}
trait Config: Default {}

pub(crate) trait Selectable<T: DrawTarget<Color = Rgb565>>: DynDrawable<T> {
    fn is_selected(&self) -> bool;
    fn set_selected(&mut self, state: bool);
}

pub(crate) struct SelectGroup<T: DrawTarget<Color = Rgb565>> {
    counter: usize,
    elements: Vec<Box<dyn Selectable<T>>>,
    selected: usize
}

impl<T: DrawTarget<Color = Rgb565>> SelectGroup<T> {
    pub fn new() -> Self {
        Self {
            counter: 0,
            elements: Vec::new(),
            selected: 0
        }
    }

    pub fn add<S: Selectable<T> + 'static>(&mut self, elem: S) {
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
}

impl<T: DrawTarget<Color = Rgb565>> DynDrawable<T> for SelectGroup<T> {
    fn draw(&self, target: &mut T) -> Result<(), T::Error> {
        for elem in self.elements.iter() {
            elem.draw(target)?;
        }
        Ok(())
    }
}
