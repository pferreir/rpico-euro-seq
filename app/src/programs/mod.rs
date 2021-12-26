use core::fmt::Debug;
use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb565};
use embedded_midi::MidiMessage;

mod converter;
mod debug;

pub use debug::DebugProgram;
pub use converter::ConverterProgram;

use crate::{ui::UIInputEvent, gate_cv::{GateCVOut, Output}};

enum ProgramName {
    Debug,
    Converter,
}

pub trait Program {
    fn new() -> Self;
    fn process_midi(&mut self, msg: &MidiMessage) {}
    fn process_ui_input(&mut self, msg: &UIInputEvent) {}

    fn render_screen<D>(&self, screen: &mut D)
    where
        D: DrawTarget<Color = Rgb565>,
        <D as DrawTarget>::Error: Debug;
    fn update_output(&self, output: &mut impl Output) {}
    fn run(&mut self, program_time: u32);
}
