use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb565};
use embedded_sdmmc::{BlockDevice, TimeSource};

use crate::{programs::Program, stdlib::TaskInterface};

use super::Overlay;

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
