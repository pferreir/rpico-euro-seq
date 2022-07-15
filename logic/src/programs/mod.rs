mod sequencer;
mod debug;

use core::{fmt::Debug, future::Future, ops::{Deref, DerefMut}, pin::Pin};
use alloc::{vec::Vec, boxed::Box, collections::BTreeMap};
use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb565, prelude::WebColors};
use embedded_midi::MidiMessage;
use embedded_sdmmc::{TimeSource, BlockDevice};
pub use sequencer::SequencerProgram;
pub use debug::DebugProgram;
use voice_lib::NotePair;

use crate::{ui::UIInputEvent, util::{GateOutput}, stdlib::{FileSystem, StdlibError, SignalId, TaskManager}};

#[derive(Debug)]
pub enum ProgramError<D: BlockDevice> {
    Stdlib(StdlibError<D>)
}

pub trait Program<'t, B: BlockDevice + 't, D: DrawTarget<Color = Rgb565>, TS: TimeSource + 't> {
    fn new() -> Self;
    fn process_midi(&mut self, msg: &MidiMessage) {}
    fn process_ui_input<'u>(&'u mut self, msg: &'u UIInputEvent) -> Result<(), ProgramError<B>> where 't: 'u, <D as DrawTarget>::Error: Debug;

    fn render_screen(&mut self, screen: &mut D);
    fn update_output<'u, 'v, T: TryFrom<&'u NotePair>>(&'v self, output: &mut impl GateOutput<'u, T>) where
    'v: 'u {}
    fn setup(&mut self);
    fn run<'u>(&mut self, program_time: u32, task_manager: impl DerefMut<Target = TaskManager<B, TS>> + 'u);
}
