use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::Size,
    mono_font::MonoTextStyle,
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::Text,
    Drawable,
};
use embedded_sdmmc::{BlockDevice, TimeSource};
use profont::PROFONT_14_POINT;

use crate::{
    screen::{SCREEN_HEIGHT, SCREEN_WIDTH},
    ui::UIInputEvent, programs::Program,
};

use super::OverlayResult;

pub trait MenuOptions {}

pub trait MenuDef<O>: Sized
where
    Self: 'static,
    O: 'static,
{
    type OptionsType;
    const OPTIONS: &'static [Self::OptionsType];

    fn label(option: &Self::OptionsType) -> &'static str;
    fn selected(&self, option: &Self::OptionsType) -> bool;
    fn run<P: Program<B, TS>, B: BlockDevice, TS: TimeSource>(
        program: &mut P,
        option: &Self::OptionsType,
    ) -> OverlayResult<O>;

    fn draw<D: DrawTarget<Color = Rgb565>>(&self, target: &mut D) -> Result<(), D::Error> {
        let text_style = MonoTextStyle::new(&PROFONT_14_POINT, Rgb565::WHITE);
        let text_style_selected = MonoTextStyle::new(&PROFONT_14_POINT, Rgb565::YELLOW);

        let window_style = PrimitiveStyleBuilder::new()
            .fill_color(Rgb565::CSS_DARK_GRAY)
            .build();
        let button_style = PrimitiveStyleBuilder::new()
            .fill_color(Rgb565::CSS_SLATE_BLUE)
            .stroke_width(1)
            .stroke_color(Rgb565::CSS_AQUAMARINE)
            .build();
        let button_style_selected = PrimitiveStyleBuilder::new()
            .fill_color(Rgb565::CSS_CORAL)
            .stroke_width(1)
            .stroke_color(Rgb565::CSS_CRIMSON)
            .build();

        let rect = Rectangle::new(
            Point::new(10, 10),
            Size::new(SCREEN_WIDTH as u32 - 20, SCREEN_HEIGHT as u32 - 20),
        );

        rect.into_styled(window_style).draw(target)?;

        let mut y = 15i32;

        for option in Self::OPTIONS {
            let text = Self::label(option);

            Rectangle::new(Point::new(15, y), Size::new(SCREEN_WIDTH as u32 - 30, 17))
                .into_styled(if self.selected(option) {
                    button_style_selected
                } else {
                    button_style
                })
                .draw(target)?;
            Text::with_alignment(
                text,
                Point::new(SCREEN_WIDTH as i32 / 2, y + 13),
                if self.selected(option) {
                    text_style_selected
                } else {
                    text_style
                },
                embedded_graphics::text::Alignment::Center,
            )
            .draw(target)?;
            y += 20;
        }
        Ok(())
    }

    fn process_ui_input<P: Program<B, TS>, B: BlockDevice, TS: TimeSource>(
        &mut self,
        program: &mut P,
        input: &UIInputEvent,
    ) -> OverlayResult<O>;
}
