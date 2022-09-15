use alloc::{boxed::Box, vec::Vec};
use core::{any::Any, fmt::Debug};
use embedded_graphics::{
    draw_target::DrawTarget,
    mono_font::MonoTextStyle,
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::Text,
};
use embedded_sdmmc::{BlockDevice, TimeSource};
use heapless::String;
use profont::PROFONT_14_POINT;

use crate::{
    programs::{Program, SequencerProgram},
    screen::{SCREEN_HEIGHT, SCREEN_WIDTH},
    stdlib::{
        ui::{
            select::{Message, SelectGroup},
            Button, ButtonId, DynDrawable, Input, Overlay, OverlayResult, UIInputEvent,
        }, StdlibError, TaskInterface, TaskType,
    },
    util::DiscreetUnwrap,
};

#[derive(Debug, PartialEq)]
enum FileLoadDialogState {
    Initializing,
    Loading,
    Loaded
}

pub(crate) struct FileLoadDialog<T: DrawTarget<Color = Rgb565>> {
    sg: SelectGroup<T>,
    file_name: String<8>,
    state: FileLoadDialogState,
}

impl<T: DrawTarget<Color = Rgb565>> Default for FileLoadDialog<T> {
    fn default() -> Self {
        Self {
            sg: SelectGroup::new(),
            file_name: String::new(),

            state: FileLoadDialogState::Initializing
        }
    }
}

impl<
        't,
        D: DrawTarget<Color = Rgb565>,
        P: Program<'t, B, D, TS, TI>,
        B: BlockDevice + 't,
        TS: TimeSource + 't,
        TI: TaskInterface + 't
    > Overlay<'t, D, P, B, TS, TI> for FileLoadDialog<D>
{
    fn process_ui_input(&mut self, _input: &UIInputEvent) -> OverlayResult<'t, D, P, B, TS, TI>
    where
        D: 't,
    {
        OverlayResult::Nop
    }

    fn draw(&self, target: &mut D) -> Result<(), <D as DrawTarget>::Error> {
        let window_style = PrimitiveStyleBuilder::new()
            .fill_color(Rgb565::CSS_DARK_GRAY)
            .build();
        let text_style_title = MonoTextStyle::new(&PROFONT_14_POINT, Rgb565::YELLOW);

        let rect = Rectangle::new(
            Point::new(10, 10),
            Size::new(SCREEN_WIDTH as u32 - 20, SCREEN_HEIGHT as u32 - 20),
        );

        // Dialog frame
        rect.into_styled(window_style).draw(target)?;

        Text::with_alignment(
            "FOOO",
            Point::new(SCREEN_WIDTH as i32 / 2, 23),
            text_style_title,
            embedded_graphics::text::Alignment::Center,
        )
        .draw(target)?;
    
        Ok(())
    }

    fn run<'u>(
        &'u mut self,
    ) -> Result<
        Option<Box<dyn FnOnce(&mut P) -> Result<Vec<TaskType>, StdlibError> + 'u>>,
        StdlibError,
    > {
        if self.state == FileLoadDialogState::Initializing {
            self.state = FileLoadDialogState::Loading;

            Ok(Some(Box::new(
                |_| {
                    let task = crate::stdlib::TaskType::DirList("data".into());
                    Ok(alloc::vec![task])
                },
            )))
        } else {
            Ok(None)
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct OKButton;

#[derive(Clone, Copy, PartialEq)]
struct CancelButton;

impl ButtonId for OKButton {
    fn clone(&self) -> Box<dyn ButtonId> {
        Box::new(OKButton)
    }

    fn eq(&self, other: &dyn ButtonId) -> bool {
        if let Some(bid) = other.as_any().downcast_ref::<OKButton>() {
            bid == self
        } else {
            false
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl ButtonId for CancelButton {
    fn clone(&self) -> Box<dyn ButtonId> {
        Box::new(CancelButton)
    }

    fn eq(&self, other: &dyn ButtonId) -> bool {
        if let Some(bid) = other.as_any().downcast_ref::<CancelButton>() {
            bid == self
        } else {
            false
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub(crate) struct FileSaveDialog<T: DrawTarget<Color = Rgb565>> {
    file_name: String<8>,
    save: bool,
    sg: SelectGroup<T>,
}

impl<T: DrawTarget<Color = Rgb565>> Default for FileSaveDialog<T> {
    fn default() -> Self {
        let mut sg = SelectGroup::new();
        sg.add(Input::new("song01", Point::new(15, 40)));
        sg.add(Button::<OKButton>::new(OKButton, "OK", Point::new(15, 65)));
        sg.add(Button::<CancelButton>::new(
            CancelButton,
            "Cancel",
            Point::new(60, 65),
        ));

        Self {
            sg,
            file_name: "song01".into(),
            save: false,
        }
    }
}

impl<'t, T: DrawTarget<Color = Rgb565> + 't, B: BlockDevice + 't, TS: TimeSource + 't, TI: TaskInterface>
    Overlay<'t, T, SequencerProgram<'t, B, TS, T, TI>, B, TS, TI> for FileSaveDialog<T>
where
    T::Error: Debug,
{
    fn process_ui_input(
        &mut self,
        input: &UIInputEvent,
    ) -> OverlayResult<'t, T, SequencerProgram<'t, B, TS, T, TI>, B, TS, TI>
    where
        T: 't,
    {
        match self.sg.process_ui_input(input) {
            Message::ButtonPress(b) => {
                if b.eq(&CancelButton) {
                    OverlayResult::Close
                } else {
                    // OK
                    self.save = true;
                    OverlayResult::Close
                }
            }
            Message::StrInput(file_name) => {
                self.file_name = file_name.into();
                OverlayResult::Nop
            }
            _ => OverlayResult::Nop,
        }
    }

    fn draw(&self, target: &mut T) -> Result<(), T::Error> {
        let text_style_title = MonoTextStyle::new(&PROFONT_14_POINT, Rgb565::YELLOW);

        let window_style = PrimitiveStyleBuilder::new()
            .fill_color(Rgb565::CSS_DARK_GRAY)
            .build();

        let rect = Rectangle::new(
            Point::new(10, 10),
            Size::new(SCREEN_WIDTH as u32 - 20, SCREEN_HEIGHT as u32 - 20),
        );

        // Dialog frame
        rect.into_styled(window_style).draw(target)?;

        // Title
        Text::with_alignment(
            "Save File",
            Point::new(SCREEN_WIDTH as i32 / 2, 23),
            text_style_title,
            embedded_graphics::text::Alignment::Center,
        )
        .draw(target)?;

        self.sg.draw(target)?;

        Ok(())
    }

    fn run<'u>(
        &'u mut self,
    ) -> Result<
        Option<Box<
            dyn FnOnce(
                    &mut SequencerProgram<'t, B, TS, T, TI>
                ) -> Result<Vec<TaskType>, StdlibError>
                + 'u,
        >>,
        StdlibError,
    > {
        if self.save {
            self.save = false;
            Ok(Some(Box::new(
                |program| {
                    let task = program.save(self.file_name.clone())?;
                    Ok(alloc::vec![task])
                },
            )))
        } else {
            Ok(None)
        }
    }
}
