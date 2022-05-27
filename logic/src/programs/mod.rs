use core::{fmt::Debug, future::Future};
use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb565};
use embedded_midi::MidiMessage;

mod sequencer;
mod debug;

use embedded_sdmmc::{TimeSource, BlockDevice};
pub use sequencer::SequencerProgram;
pub use debug::DebugProgram;
use voice_lib::NotePair;

use crate::{ui::UIInputEvent, util::{GateOutput}, stdlib::{FileSystem, StdlibError}};

#[derive(Debug)]
pub enum ProgramError<D: BlockDevice> {
    Stdlib(StdlibError<D>)
}

pub trait Program<B: BlockDevice, TS: TimeSource> {
    type SetupFuture<'a>: Future<Output = Result<(), ProgramError<B>>> + 'a
    where
        Self: 'a;

    fn new(fs: FileSystem<B, TS>) -> Self;
    fn process_midi(&mut self, msg: &MidiMessage) {}
    fn process_ui_input(&mut self, msg: &UIInputEvent) {}

    fn render_screen<D>(&self, screen: &mut D)
    where
        D: DrawTarget<Color = Rgb565>,
        <D as DrawTarget>::Error: Debug;
    fn update_output<'u, 'v, T: From<&'u NotePair>>(&'v self, output: &mut impl GateOutput<'u, T>) where
    'v: 'u {}
    fn setup<'a>(&'a mut self) -> Self::SetupFuture<'a>;
    fn run(&mut self, program_time: u32);
}
