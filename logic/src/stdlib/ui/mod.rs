#[macro_use]
pub mod icons;
pub mod select;

mod button;
mod dialog;
mod input;
mod menu;

use alloc::boxed::Box;
pub use button::{Button, ButtonId};
pub use dialog::Dialog;
use embedded_graphics::{
    draw_target::DrawTarget, pixelcolor::Rgb565
};
use embedded_sdmmc::{BlockDevice, TimeSource};
pub use input::Input;
pub use menu::{MenuDef, MenuOptions};
use ufmt::derive::uDebug;

use crate::{programs::Program};

use super::{SignalId, StdlibError, TaskInterface};


#[derive(uDebug, Debug, Clone)]
pub enum UIInputEvent {
    EncoderTurn(i8),
    EncoderSwitch(bool),
    Switch1(bool),
    Switch2(bool)
}

pub trait DynTarget {}

pub trait DynDrawable<T: DrawTarget<Color = Rgb565>> {
    fn draw(&self, target: &mut T) -> Result<(), T::Error>;
}

pub trait Overlay<
    't,
    D: DrawTarget<Color = Rgb565>,
    P: Program<'t, B, D, TS, TI>,
    B: BlockDevice + 't,
    TS: TimeSource + 't,
    TI: TaskInterface + 't,
>
{
    fn process_ui_input(&mut self, input: &UIInputEvent) -> OverlayResult<'t, D, P, B, TS, TI>
    where
        D: 't;

    fn run<'u>(
        &'u mut self,
    ) -> Result<
        Option<Box<dyn FnOnce(&mut P, &mut TI) -> Result<(), StdlibError> + 'u>>,
        StdlibError,
    >;
    fn draw(&self, target: &mut D) -> Result<(), D::Error>;
}

pub enum OverlayResult<
    't,
    D: DrawTarget<Color = Rgb565>,
    P: Program<'t, B, D, TS, TI>,
    B: BlockDevice + 't,
    TS: TimeSource + 't,
    TI: TaskInterface + 't,
> {
    Nop,
    Push(Box<dyn Overlay<'t, D, P, B, TS, TI> + 't>),
    Replace(Box<dyn Overlay<'t, D, P, B, TS, TI> + 't>),
    CloseOnSignal(SignalId),
    Close,
}
