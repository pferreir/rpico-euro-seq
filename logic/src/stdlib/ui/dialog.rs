use alloc::boxed::Box;
use embedded_graphics::{prelude::WebColors, draw_target::DrawTarget, pixelcolor::Rgb565};
use embedded_sdmmc::{BlockDevice, TimeSource};

use crate::{programs::Program, ui::UIInputEvent};

use super::{OverlayResult, Overlay};

pub trait Dialog<'t, DT: DrawTarget<Color=Rgb565>,  P: Program<'t, B, DT, TS>, B: BlockDevice + 't, TS: TimeSource + 't>: Overlay<'t, DT, P, B, TS> {
    fn draw(&self, target: &mut DT) -> Result<(), DT::Error>;
}
