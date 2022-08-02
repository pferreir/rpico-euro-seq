use alloc::boxed::Box;
use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb565, prelude::WebColors};
use embedded_sdmmc::{BlockDevice, TimeSource};

use crate::{programs::Program, ui::UIInputEvent, stdlib::TaskInterface};

use super::{Overlay, OverlayResult};

pub trait Dialog<
    't,
    DT: DrawTarget<Color = Rgb565>,
    P: Program<'t, B, DT, TS, TI>,
    B: BlockDevice + 't,
    TS: TimeSource + 't,
    TI: TaskInterface + 't,
>: Overlay<'t, DT, P, B, TS, TI>
{
    fn draw(&self, target: &mut DT) -> Result<(), DT::Error>;
}
