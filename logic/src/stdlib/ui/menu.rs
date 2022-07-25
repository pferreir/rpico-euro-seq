use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb565};
use embedded_sdmmc::{BlockDevice, TimeSource};

use crate::{programs::Program, ui::UIInputEvent};

use super::{Overlay, OverlayResult};

pub trait MenuOptions {}

pub trait MenuDef<
    't,
    D: DrawTarget<Color = Rgb565>,
    P: Program<'t, B, D, TS>,
    B: BlockDevice + 't,
    TS: TimeSource + 't,
>: Overlay<'t, D, P, B, TS>
{
    type OptionType;

    fn options(&self) -> &'t [Self::OptionType];
    fn label(&self, option: &Self::OptionType) -> &'static str;
    fn selected(&self, option: &Self::OptionType) -> bool;

    fn run_choice(option: &Self::OptionType) -> OverlayResult<'t, D, P, B, TS>
    where
        D: 't;

    fn process_ui_input(&mut self, input: &UIInputEvent) -> OverlayResult<'t, D, P, B, TS>
    where
        D: 't;
}

#[macro_export]
macro_rules! impl_overlay {
    ($t: ident, $p: ident) => {
        use crate::screen::{SCREEN_HEIGHT, SCREEN_WIDTH};
        use crate::stdlib::{ui::Overlay, StdlibError, TaskInterface};
        use embedded_graphics::{
            mono_font::MonoTextStyle,
            primitives::{PrimitiveStyleBuilder, Rectangle},
            text::{Alignment, Text},
        };
        use profont::PROFONT_14_POINT;

        impl<'t, D: DrawTarget<Color = Rgb565> + 't, B: BlockDevice + 't, TS: TimeSource + 't>
            Overlay<'t, D, $p<'t, B, TS, D>, B, TS> for $t
        where
            D::Error: Debug,
        {
            fn draw(&self, target: &mut D) -> Result<(), D::Error> {
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

                for option in <Self as MenuDef<'t, D, $p<'t, B, TS, D>, _, _>>::options(self) {
                    let text =
                        <Self as MenuDef<'t, D, $p<'t, B, TS, D>, _, _>>::label(self, option);

                    Rectangle::new(Point::new(15, y), Size::new(SCREEN_WIDTH as u32 - 30, 17))
                        .into_styled(
                            if <Self as MenuDef<'t, D, $p<'t, B, TS, D>, _, _>>::selected(
                                self, option,
                            ) {
                                button_style_selected
                            } else {
                                button_style
                            },
                        )
                        .draw(target)?;
                    Text::with_alignment(
                        text,
                        Point::new(SCREEN_WIDTH as i32 / 2, y + 13),
                        if <Self as MenuDef<'t, D, $p<'t, B, TS, D>, _, _>>::selected(self, option)
                        {
                            text_style_selected
                        } else {
                            text_style
                        },
                        Alignment::Center,
                    )
                    .draw(target)?;
                    y += 20;
                }
                Ok(())
            }

            fn process_ui_input(
                &mut self,
                input: &UIInputEvent,
            ) -> OverlayResult<'t, D, $p<'t, B, TS, D>, B, TS>
            where
                D: 't,
            {
                <Self as MenuDef<'t, D, $p<'t, B, TS, D>, B, TS>>::process_ui_input(self, input)
            }

            fn run<'u>(
                &'u mut self,
            ) -> Result<
                Option<Box<dyn FnOnce(&mut $p<'t, B, TS, D>, &mut TaskInterface) -> Result<(), StdlibError<B>>>>,
                StdlibError<B>,
            >
            {
                Ok(None)
            }
        }
    };
}
