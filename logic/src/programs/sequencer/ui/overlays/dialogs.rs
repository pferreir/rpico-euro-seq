use core::{arch::wasm::unreachable, marker::PhantomData};

use alloc::boxed::Box;
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
use profont::{PROFONT_12_POINT, PROFONT_14_POINT};

use crate::{
    programs::{Program, SequencerProgram},
    screen::{SCREEN_HEIGHT, SCREEN_WIDTH},
    stdlib::{
        ui::{select::SelectGroup, Button, Dialog, Input, Overlay, OverlayResult, DynDrawable},
        File,
    },
    ui::UIInputEvent,
};

pub(crate) struct FileLoadDialog<T: DrawTarget<Color = Rgb565>> {
    sg: SelectGroup<T>,
    file_name: String<12>,
}

impl<T: DrawTarget<Color = Rgb565>> Default for FileLoadDialog<T> {
    fn default() -> Self {
        let mut sg = SelectGroup::new();

        Self {
            sg,
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
        program: &mut P
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

pub(crate) struct FileSaveDialog<T: DrawTarget<Color = Rgb565>> {
    sg: SelectGroup<T>,
    file_name: String<12>,
}

impl<T: DrawTarget<Color = Rgb565>> Default for FileSaveDialog<T> {
    fn default() -> Self {
        let mut sg = SelectGroup::new();
        sg.add(Input::new("song01", Point::new(15, 40)));
        sg.add(Button::new("OK", Point::new(15, 65)));
        sg.add(Button::new("Cancel", Point::new(60, 65)));

        Self {
            sg,
            file_name: String::new(),
        }
    }
}

impl<'t, T: DrawTarget<Color = Rgb565>,  P: Program<'t, B, TS, T>, B: BlockDevice, TS: TimeSource> Overlay<'t, T, P, B, TS> for FileSaveDialog<T> {
    fn process_ui_input(
        &mut self,
        input: &UIInputEvent,
        program: &mut P
    ) -> OverlayResult<'t, T, P, B, TS>
    where
        T: 't,
    {
        match input {
            UIInputEvent::EncoderTurn(v) => {
                // self.selection += v;
                self.sg.change(*v);
                OverlayResult::Nop
            }
            UIInputEvent::EncoderSwitch(true) => OverlayResult::Nop,
            _ => OverlayResult::Nop,
        }
    }

    fn draw(&self, target: &mut T) -> Result<(), T::Error> {
        let text_style = MonoTextStyle::new(&PROFONT_12_POINT, Rgb565::WHITE);
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
