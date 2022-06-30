mod dialogs;
mod menus;

use core::fmt::Debug;

use alloc::boxed::Box;
use embedded_graphics::{
    draw_target::DrawTarget, pixelcolor::Rgb565, prelude::WebColors, Drawable,
};
use embedded_sdmmc::{BlockDevice, TimeSource};
use heapless::String;

use crate::{
    programs::{Program, SequencerProgram},
    stdlib::ui::OverlayResult,
    ui::UIInputEvent,
    util::DiscreetUnwrap,
};

pub(crate) use self::menus::FileMenu;

impl<'t, B: BlockDevice, TS: TimeSource, D: DrawTarget<Color = Rgb565>>
    SequencerProgram<'t, B, TS, D>
where
    <D as DrawTarget>::Error: Debug,
{
    pub(crate) fn _draw_overlays(&self, screen: &mut D) {
        for overlay in self.overlays.iter() {
            overlay.draw(screen).duwrp();
        }
    }
}
