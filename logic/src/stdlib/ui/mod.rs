#[macro_use]
pub mod icons;
pub mod select;

mod button;
mod dialog;
mod input;
mod menu;
mod overlays;

pub use button::{Button, ButtonId};
pub use dialog::Dialog;
use embedded_graphics::{
    draw_target::DrawTarget, pixelcolor::Rgb565
};
pub use input::Input;
pub use menu::{MenuDef, MenuOptions};
pub use overlays::{Overlay, OverlayResult, OverlayManager};
use ufmt::derive::uDebug;



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
