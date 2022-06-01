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
    programs::SequencerProgram,
    screen::{SCREEN_HEIGHT, SCREEN_WIDTH},
    stdlib::ui::{select::{SelectGroup}, Input, Button, OverlayResult},
    ui::UIInputEvent,
};

pub(crate) enum Dialog {
    FileSave(FileSaveDialog),
    FileLoad(FileLoadDialog),
}

impl Dialog {
    pub(crate) fn draw<D: DrawTarget<Color = Rgb565>>(
        &self,
        target: &mut D,
    ) -> Result<(), D::Error> {
        match self {
            Dialog::FileSave(s) => s.draw(target),
            Dialog::FileLoad(l) => l.draw(target),
        }
    }

    pub(crate) fn process_ui_input<B: BlockDevice, TS: TimeSource, O>(
        &mut self,
        program: &mut SequencerProgram<B, TS>,
        input: &UIInputEvent,
    ) -> OverlayResult<O> {
        match self {
            Dialog::FileSave(s) => s.process_ui_input(program, input),
            Dialog::FileLoad(l) => l.process_ui_input(program, input),
        }
    }
}

trait DialogDef {
    fn draw<D: DrawTarget<Color = Rgb565>>(&self, target: &mut D) -> Result<(), D::Error>;
    fn process_ui_input<B: BlockDevice, TS: TimeSource, O>(
        &mut self,
        program: &mut SequencerProgram<B, TS>,
        input: &UIInputEvent,
    ) -> OverlayResult<O>;
}

pub(crate) struct FileSaveDialog {
    file_name: String<16>,
    selection: i8,
}

impl Default for FileSaveDialog {
    fn default() -> Self {
        Self {
            file_name: String::new(),
            selection: 0,
        }
    }
}

pub(crate) struct FileLoadDialog;

impl DialogDef for FileLoadDialog {
    fn draw<D: DrawTarget<Color = Rgb565>>(&self, target: &mut D) -> Result<(), D::Error> {
        todo!()
    }

    fn process_ui_input<B: BlockDevice, TS: TimeSource, O>(
        &mut self,
        program: &mut SequencerProgram<B, TS>,
        input: &UIInputEvent,
    ) -> OverlayResult<O> {
        todo!()
    }
}

impl DialogDef for FileSaveDialog {
    fn draw<D: DrawTarget<Color = Rgb565>>(&self, target: &mut D) -> Result<(), D::Error> {
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

        let mut sg: SelectGroup<3, _, _> = SelectGroup::new(target, self.selection as isize);

        sg.add(Input::new("filename.sng", Point::new(15, 40)))?;
        sg.add(Button::new("OK", Point::new(15, 65)))?;
        sg.add(Button::new("Cancel", Point::new(60, 65)))?;
        Ok(())
    }

    fn process_ui_input<B: BlockDevice, TS: TimeSource, O>(
        &mut self,
        program: &mut SequencerProgram<B, TS>,
        input: &UIInputEvent,
    ) -> OverlayResult<O> {
        match input {
            UIInputEvent::EncoderTurn(v) => {
                self.selection += v;
                OverlayResult::Nop
            }
            UIInputEvent::EncoderSwitch(true) => OverlayResult::Nop,
            _ => OverlayResult::Nop,
        }
    }
}
