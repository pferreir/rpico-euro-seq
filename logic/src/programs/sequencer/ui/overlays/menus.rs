use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::Rgb565,
    prelude::*,
    Drawable,
};
use embedded_sdmmc::{BlockDevice, TimeSource};

use crate::{
    log,
    programs::{SequencerProgram, Program},
    ui::UIInputEvent,
    util::DiscreetUnwrap, stdlib::ui::{MenuOptions, MenuDef, OverlayResult},
};

use super::{dialogs::{FileSaveDialog, FileLoadDialog, Dialog}, Overlay};

pub(crate) enum Menu {
    File(FileMenu),
}

impl Menu {
    pub(super) fn draw<D: DrawTarget<Color = Rgb565>>(&self, target: &mut D) -> Result<(), D::Error> {
        match self {
            Menu::File(f) => f.draw(target),
            _ => unimplemented!(),
        }
    }

    pub fn process_ui_input<B: BlockDevice, TS: TimeSource>(
        &mut self,
        program: &mut SequencerProgram<B, TS>,
        input: &UIInputEvent,
    ) -> OverlayResult<Overlay> {
        match self {
            Menu::File(f) => f.process_ui_input(program, input),
            _ => unimplemented!(),
        }
    }
}

pub(crate) struct FileMenu {
    selection: FileMenuOption,
}

impl Default for FileMenu {
    fn default() -> Self {
        Self {
            selection: FileMenuOption::Load,
        }
    }
}

pub(crate) struct FileMenuOptionError;

#[derive(Clone, Copy, PartialEq)]
#[repr(u8)]
pub(crate) enum FileMenuOption {
    Load = 0,
    Save = 1,
    Cancel = 2,
}

impl TryFrom<i8> for FileMenuOption {
    type Error = FileMenuOptionError;

    fn try_from(val: i8) -> Result<Self, Self::Error> {
        Ok(match val {
            0 => FileMenuOption::Load,
            1 => FileMenuOption::Save,
            2 => FileMenuOption::Cancel,
            _ => return Err(FileMenuOptionError),
        })
    }
}

impl MenuOptions for FileMenu {}

impl MenuDef<Overlay> for FileMenu
{
    type OptionsType = FileMenuOption;
    const OPTIONS: &'static [FileMenuOption] = &[
        FileMenuOption::Load,
        FileMenuOption::Save,
        FileMenuOption::Cancel,
    ];

    fn label(option: &Self::OptionsType) -> &'static str {
        match option {
            FileMenuOption::Load => "Load",
            FileMenuOption::Save => "Save",
            FileMenuOption::Cancel => "Cancel",
        }
    }

    fn selected(&self, option: &Self::OptionsType) -> bool {
        self.selection == *option
    }

    fn run<P: Program<B, TS>, B: BlockDevice, TS: TimeSource>(
        program: &mut P,
        option: &Self::OptionsType,
    ) -> OverlayResult<Overlay> {
        match option {
            FileMenuOption::Load => {
                log::info("CHOSE 'LOAD'");
                OverlayResult::Push(Overlay::Dialog(Dialog::FileLoad(FileLoadDialog)))
            }
            FileMenuOption::Save => {
                log::info("CHOSE 'SAVE'");
                OverlayResult::Push(Overlay::Dialog(Dialog::FileSave(FileSaveDialog::default())))
            }
            FileMenuOption::Cancel => {
                log::info("CHOSE 'CANCEL'");
                OverlayResult::Close
            }
        }
    }

    fn process_ui_input<P: Program<B, TS>, B: BlockDevice, TS: TimeSource>(
        &mut self,
        program: &mut P,
        input: &UIInputEvent,
    ) -> OverlayResult<Overlay> {
        match input {
            UIInputEvent::EncoderTurn(v) => {
                self.selection = (self.selection as i8 + v)
                    .rem_euclid(Self::OPTIONS.len() as i8)
                    .try_into()
                    .duwrp();
                OverlayResult::Nop
            }
            UIInputEvent::EncoderSwitch(true) => {
                Self::run(program, &self.selection)
            }
            _ => OverlayResult::Nop
        }
    }
}
