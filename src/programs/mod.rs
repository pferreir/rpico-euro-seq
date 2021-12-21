use core::fmt::Debug;
use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb565};
use embedded_midi::MidiMessage;

mod debug;

pub use debug::DebugProgram;

enum ProgramName {
    Debug,
}

pub trait Program {
    fn new() -> Self;
    fn process_midi(&mut self, msg: &MidiMessage) {}

    fn render_screen<D>(&self, screen: &mut D)
    where
        D: DrawTarget<Color = Rgb565>,
        <D as DrawTarget>::Error: Debug;
    fn run(&mut self, program_time: u64);
}
