use core::fmt::Debug;

use alloc::boxed::Box;
use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb565, prelude::*, Drawable};
use embedded_sdmmc::{BlockDevice, TimeSource};

use crate::{
    impl_overlay, log,
    programs::{Program, SequencerProgram},
    stdlib::ui::{MenuDef, MenuOptions, OverlayResult},
    ui::UIInputEvent,
    util::DiscreetUnwrap,
};

use super::dialogs::{FileLoadDialog, FileSaveDialog};

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

impl_overlay!(FileMenu, SequencerProgram);

impl<'t, D: DrawTarget<Color = Rgb565>, B: BlockDevice, TS: TimeSource>
    MenuDef<'t, D, SequencerProgram<'t, B, TS, D>, B, TS> for FileMenu
where
    D::Error: Debug,
{
    type OptionType = FileMenuOption;

    fn options(&self) -> &'t [Self::OptionType]
    where
        Self: Sized,
    {
        &[
            FileMenuOption::Load,
            FileMenuOption::Save,
            FileMenuOption::Cancel,
        ]
    }
    fn label(&self, option: &FileMenuOption) -> &'static str {
        match option {
            FileMenuOption::Load => "Load",
            FileMenuOption::Save => "Save",
            FileMenuOption::Cancel => "Cancel",
        }
    }

    fn selected(&self, option: &FileMenuOption) -> bool {
        self.selection == *option
    }

    fn run(
        program: &mut SequencerProgram<'t, B, TS, D>,
        option: &FileMenuOption,
    ) -> OverlayResult<'t, D, SequencerProgram<'t, B, TS, D>, B, TS>
    where
        D: 't,
    {
        match option {
            FileMenuOption::Load => {
                log::info("CHOSE 'LOAD'");
                OverlayResult::Push(Box::new(FileLoadDialog::<D>::default()))
            }
            FileMenuOption::Save => {
                log::info("CHOSE 'SAVE'");
                OverlayResult::Push(Box::new(FileSaveDialog::default()))
            }
            FileMenuOption::Cancel => {
                log::info("CHOSE 'CANCEL'");
                OverlayResult::Close
            }
        }
    }

    fn process_ui_input(
        &mut self,
        input: &UIInputEvent,
        program: &mut SequencerProgram<'t, B, TS, D>,
    ) -> OverlayResult<'t, D, SequencerProgram<'t, B, TS, D>, B, TS>
    where
        D: 't,
    {
        match input {
            UIInputEvent::EncoderTurn(v) => {
                self.selection = (self.selection as i8 + *v)
                    .rem_euclid(
                        <Self as MenuDef<'t, D, SequencerProgram<'t, B, TS, D>, B, TS>>::options(
                            self,
                        )
                        .len() as i8,
                    )
                    .try_into()
                    .duwrp();
                OverlayResult::Nop
            }
            UIInputEvent::EncoderSwitch(true) => Self::run(program, &self.selection),
            _ => OverlayResult::Nop,
        }
    }
}
