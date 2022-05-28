use embedded_graphics::{
    draw_target::DrawTarget,
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    text::Text,
    Drawable,
};
use embedded_sdmmc::{BlockDevice, TimeSource};
use heapless::{String, Vec};

use crate::{log, programs::SequencerProgram, util::DiscreteUnwrap};

trait MenuOptions {}

trait MenuDef: Sized
where
    Self: 'static,
{
    const Options: &'static [Self];

    fn label(&self) -> &'static str;
    fn run(&self);
}

pub(crate) enum Menu {
    File(FileMenu),
}

impl Menu {
    fn _draw_menu<D: DrawTarget<Color = Rgb565>, M: MenuDef>(&self, target: &mut D, menu: &M) {
        let style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);

        for option in M::Options {
            let text = option.label();

            Text::new(text, Point::new(20, 20), style)
                .draw(target)
                .duwrp();
        }
    }

    fn draw<D: DrawTarget<Color = Rgb565>>(&self, target: &mut D) {
        match self {
            Menu::File(f) => self._draw_menu(target, f),
            _ => unimplemented!(),
        }
    }
}

pub(crate) enum Overlay {
    Menu(Menu),
    Alert(String<128>),
}

pub(crate) enum FileMenu {
    Load,
    Save,
    Cancel,
}

impl MenuOptions for FileMenu {}

impl MenuDef for FileMenu {
    const Options: &'static [Self] = &[Self::Load, Self::Save, Self::Cancel];

    fn label(&self) -> &'static str {
        match self {
            FileMenu::Load => "Load",
            FileMenu::Save => "Save",
            FileMenu::Cancel => "Cancel",
        }
    }

    fn run(&self) {
        match self {
            FileMenu::Load => {
                log::info("CHOSE 'LOAD'");
            }
            FileMenu::Save => {
                log::info("CHOSE 'SAVE'");
            }
            FileMenu::Cancel => {
                log::info("CHOSE 'CANCEL'");
            }
        }
    }
}

impl<'t, B: BlockDevice, TS: TimeSource> SequencerProgram<'t, B, TS> {
    pub(crate) fn _draw_overlays<D>(&self, screen: &mut D)
    where
        D: DrawTarget<Color = Rgb565>
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
        D: DrawTarget<Color = Self::Color>
    {
        match self {
            Overlay::Menu(m) => m.draw::<D>(target),
            _ => unimplemented!(),
        };
        Ok(())
    }
}

impl Overlay {}
