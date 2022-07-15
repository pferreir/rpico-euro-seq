use core::fmt::Debug;

use embedded_graphics::{
    draw_target::DrawTarget,
    image::Image,
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
};
use embedded_sdmmc::{BlockDevice, TimeSource};

use crate::{programs::{sequencer::State, SequencerProgram}, util::DiscreetUnwrap};

pub(crate) mod icons {
    pub(crate) static PLAY_ICON: &[u8] = include_bytes!("../../../../assets/play.bmp");
    pub(crate) static PAUSE_ICON: &[u8] = include_bytes!("../../../../assets/pause.bmp");
    pub(crate) static RECORD_ICON: &[u8] = include_bytes!("../../../../assets/record.bmp");
    pub(crate) static RECORD_ON_ICON: &[u8] = include_bytes!("../../../../assets/record_on.bmp");
    pub(crate) static STOP_ICON: &[u8] = include_bytes!("../../../../assets/stop.bmp");
    pub(crate) static STOP_ON_ICON: &[u8] = include_bytes!("../../../../assets/stop_on.bmp");
    pub(crate) static BEGINNING_ICON: &[u8] = include_bytes!("../../../../assets/beginning.bmp");
    pub(crate) static SEEK_ICON: &[u8] = include_bytes!("../../../../assets/seek.bmp");
}

pub(crate) const NUM_UI_ACTIONS: usize = 5;

#[derive(Copy, Clone)]
#[repr(u8)]
pub(crate) enum UIAction {
    PlayPause = 0,
    Stop = 1,
    Record = 2,
    Beginning = 3,
    Seek = 4,
}

impl UIAction {
    fn button_pos(&self) -> Point {
        match self {
            UIAction::PlayPause => Point::new(0, 0),
            UIAction::Stop => Point::new(25, 0),
            UIAction::Record => Point::new(50, 0),
            UIAction::Beginning => Point::new(80, 0),
            UIAction::Seek => Point::new(105, 0),
        }
    }
}

impl From<u8> for UIAction {
    fn from(v: u8) -> Self {
        match v % NUM_UI_ACTIONS as u8 {
            0 => UIAction::PlayPause,
            1 => UIAction::Stop,
            2 => UIAction::Record,
            3 => UIAction::Beginning,
            4 => UIAction::Seek,
            _ => unreachable!(),
        }
    }
}

impl<'t, B: BlockDevice, TS: TimeSource, D: DrawTarget<Color = Rgb565>> SequencerProgram<'t, B, TS, D> where
    <D as DrawTarget>::Error: Debug,
{
    pub(crate) fn draw_buttons(&self, pos: Point, screen: &mut D)
    where
        D: DrawTarget<Color = Rgb565>,
    {
        Image::new(
            if let State::Playing(_, _) = self.state {
                &self.pause_icon
            } else {
                &self.play_icon
            },
            pos + UIAction::PlayPause.button_pos(),
        )
        .draw(screen)
        .duwrp();
        Image::new(
            if let State::Stopped = self.state {
                &self.stop_on_icon
            } else {
                &self.stop_icon
            },
            pos + UIAction::Stop.button_pos(),
        )
        .draw(screen)
        .duwrp();
        Image::new(
            if let State::Recording(_, _) = self.state {
                &self.record_on_icon
            } else {
                &self.record_icon
            },
            pos + UIAction::Record.button_pos(),
        )
        .draw(screen)
        .duwrp();
        Image::new(&self.beginning_icon, pos + UIAction::Beginning.button_pos())
            .draw(screen)
            .duwrp();
        Image::new(&self.seek_icon, pos + UIAction::Seek.button_pos())
            .draw(screen)
            .duwrp();

        Rectangle::new(pos + self.selected_action.button_pos(), Size::new(26, 16))
            .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 1))
            .draw(screen)
            .duwrp();
    }
}
