pub(crate) mod select;

mod button;
mod dialog;
mod input;
mod menu;

use core::any::Any;

use alloc::boxed::Box;
pub use button::{Button, ButtonId};
pub use dialog::{Dialog};
use embedded_graphics::{prelude::WebColors, draw_target::DrawTarget, Drawable, pixelcolor::{Rgb565}};
use embedded_sdmmc::{BlockDevice, TimeSource};
pub use input::Input;
pub use menu::{MenuDef, MenuOptions};

use crate::{programs::Program, ui::UIInputEvent};

pub trait DynTarget {}

pub trait DynDrawable<T: DrawTarget<Color = Rgb565>> {
    fn draw(&self, target: &mut T) -> Result<(), T::Error>;
}

pub trait Overlay<'t, D: DrawTarget<Color = Rgb565>, P: Program<'t, B, TS, D>, B: BlockDevice, TS: TimeSource> {
    fn process_ui_input(
        &mut self,
        input: &UIInputEvent,
        program: &mut P
    ) -> OverlayResult<'t, D, P, B, TS> where D: 't;

    fn draw(&self, target: &mut D) -> Result<(), D::Error>;
}

pub enum OverlayResult<'t, D: DrawTarget<Color = Rgb565>, P: Program<'t, B, TS, D>, B: BlockDevice, TS: TimeSource> {
    Nop,
    Push(Box<dyn Overlay<'t, D, P, B, TS> + 't>),
    Replace(Box<dyn Overlay<'t, D, P, B, TS> + 't>),
    Close
}
