pub(crate) mod select;

mod button;
mod dialog;
mod input;
mod menu;

use core::{any::Any, future::Future, pin::Pin};

use alloc::boxed::Box;
pub use button::{Button, ButtonId};
pub use dialog::{Dialog};
use embedded_graphics::{prelude::WebColors, draw_target::DrawTarget, Drawable, pixelcolor::{Rgb565}};
use embedded_sdmmc::{BlockDevice, TimeSource};
use futures::channel::mpsc;
pub use input::Input;
pub use menu::{MenuDef, MenuOptions};

use crate::{programs::{Program}, ui::UIInputEvent};

use super::{TaskManager, SignalId, StdlibError, Task};


pub trait DynTarget {}

pub trait DynDrawable<T: DrawTarget<Color = Rgb565>> {
    fn draw(&self, target: &mut T) -> Result<(), T::Error>;
}

pub trait Overlay<'t, D: DrawTarget<Color = Rgb565>, P: Program<'t, B, D, TS>, B: BlockDevice + 't, TS: TimeSource + 't> {
    fn process_ui_input(
        &mut self,
        input: &UIInputEvent,
    ) -> OverlayResult<'t, D, P, B, TS> where D: 't;

    fn run<'u>(&'u mut self) -> Result<Option<Box<dyn FnOnce(&mut P, &mut mpsc::Sender<Task>) -> Result<(), StdlibError<B>> + 'u>>, StdlibError<B>>;
    fn draw(&self, target: &mut D) -> Result<(), D::Error>;
}

pub enum OverlayResult<'t, D: DrawTarget<Color = Rgb565>, P: Program<'t, B, D, TS>, B: BlockDevice + 't, TS: TimeSource + 't> {
    Nop,
    Push(Box<dyn Overlay<'t, D, P, B, TS> + 't>),
    Replace(Box<dyn Overlay<'t, D, P, B, TS> + 't>),
    CloseOnSignal(SignalId),
    Close
}
