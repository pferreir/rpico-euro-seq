mod dialogs;
mod menus;

use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::Rgb565,
    Drawable,
};
use embedded_sdmmc::{BlockDevice, TimeSource};
use heapless::String;

use crate::{
    programs::SequencerProgram,
    ui::UIInputEvent,
    util::DiscreetUnwrap, stdlib::ui::OverlayResult,
};

use self::dialogs::Dialog;

pub(crate) use self::menus::{Menu, FileMenu};

pub(crate) enum Overlay {
    Menu(Menu),
    Dialog(Dialog),
    Alert(String<128>),
}


impl<'t, B: BlockDevice, TS: TimeSource> SequencerProgram<'t, B, TS> {
    pub(crate) fn _draw_overlays<D>(&self, screen: &mut D)
    where
        D: DrawTarget<Color = Rgb565>,
    {
        for overlay in self.overlays.iter() {
            overlay.draw(screen).duwrp();
        }
    }
}

impl Drawable for Overlay {
    type Color = Rgb565;
    type Output = ();

    fn draw<D>(&self, target: &mut D) -> Result<Self::Output, D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        match self {
            Overlay::Menu(m) => m.draw::<D>(target),
            Overlay::Dialog(d) => d.draw::<D>(target),
            _ => todo!(),
        }
    }
}

impl Overlay {
    pub fn process_ui_input<B: BlockDevice, TS: TimeSource>(
        &mut self,
        program: &mut SequencerProgram<B, TS>,
        input: &UIInputEvent,
    ) -> OverlayResult<Overlay> {
        match self {
            Overlay::Menu(m) => m.process_ui_input(program, input),
            Overlay::Dialog(d) => d.process_ui_input(program, input),
            _ => unimplemented!(),
        }
    }
}
