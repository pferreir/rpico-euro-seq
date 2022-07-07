use core::{any::Any, fmt::Debug};
use alloc::{boxed::Box, string::ToString};
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
use profont::{PROFONT_14_POINT};

use crate::{
    programs::{Program, SequencerProgram},
    screen::{SCREEN_HEIGHT, SCREEN_WIDTH},
    stdlib::{
        ui::{select::{SelectGroup, Message}, Button, Dialog, DynDrawable, Input, Overlay, OverlayResult, ButtonId},
    },
    ui::UIInputEvent,
};

enum FileLoadAction {

}

pub(crate) struct FileLoadDialog<T: DrawTarget<Color = Rgb565>> {
    sg: SelectGroup<T>,
    file_name: String<12>,
}

impl<T: DrawTarget<Color = Rgb565>> Default for FileLoadDialog<T> {
    fn default() -> Self {
        Self {
            sg: SelectGroup::new(),
            file_name: String::new(),
        }
    }
}

impl<
        't,
        D: DrawTarget<Color = Rgb565>,
        P: Program<'t, B, TS, D>,
        B: BlockDevice,
        TS: TimeSource,
    > Overlay<'t, D, P, B, TS> for FileLoadDialog<D>
{
    fn process_ui_input(
        &mut self,
        input: &UIInputEvent,
        program: &mut P,
    ) -> OverlayResult<'t, D, P, B, TS>
    where
        D: 't,
    {
        OverlayResult::Nop
    }

    fn draw(&self, target: &mut D) -> Result<(), <D as DrawTarget>::Error> {
        todo!()
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
    file_name: String<12>,
    sg: SelectGroup<T>,
}

impl<T: DrawTarget<Color = Rgb565>> Default for FileSaveDialog<T> {
    fn default() -> Self {
        let mut sg = SelectGroup::new();
        sg.add(Input::new("song01", Point::new(15, 40)));
        sg.add(Button::<OKButton>::new(OKButton, "OK", Point::new(15, 65)));
        sg.add(Button::<CancelButton>::new(CancelButton, "Cancel", Point::new(60, 65)));

        Self { sg, file_name: "song01".into() }
    }
}

impl<
        't,
        T: DrawTarget<Color = Rgb565>,
        B: BlockDevice,
        TS: TimeSource,
    > Overlay<'t, T, SequencerProgram<'t, B, TS, T>, B, TS> for FileSaveDialog<T> where
    T::Error: Debug
{
    fn process_ui_input(
        &mut self,
        input: &UIInputEvent,
        program: &mut SequencerProgram<'t, B, TS, T>,
    ) -> OverlayResult<'t, T, SequencerProgram<'t, B, TS, T>, B, TS>
    where
        T: 't,
    {

        match self.sg.process_ui_input(input) {
            Message::ButtonPress(b) => {
                if b.eq(&CancelButton) {
                    OverlayResult::Close
                } else {
                    // OK
                    program.save(&self.file_name);
                    OverlayResult::Close
                }
            },
            Message::StrInput(file_name) => {
                self.file_name = file_name.into();
                OverlayResult::Nop
            }
            _ => {
                OverlayResult::Nop
            }
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
}
