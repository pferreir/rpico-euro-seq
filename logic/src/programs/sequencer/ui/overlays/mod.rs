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
    stdlib::{ui::OverlayResult, TaskManager},
    ui::UIInputEvent,
    util::DiscreetUnwrap,
};

pub(crate) use self::menus::FileMenu;

