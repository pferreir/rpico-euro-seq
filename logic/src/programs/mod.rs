mod debug;
mod sequencer;

use core::{
    fmt::Debug,
    ops::{Deref, DerefMut},
};
pub use debug::DebugProgram;
use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb565};
use embedded_midi::MidiMessage;
use embedded_sdmmc::{BlockDevice, TimeSource};
pub use sequencer::SequencerProgram;
use voice_lib::NotePair;

use crate::stdlib::{ui::UIInputEvent, Output, StdlibError, TaskInterface};

#[derive(Debug)]
pub enum ProgramError {
    Stdlib(StdlibError),
}

impl From<StdlibError> for ProgramError {
    fn from(err: StdlibError) -> Self {
        ProgramError::Stdlib(err)
    }
}

pub trait Program<
    't,
    B: BlockDevice + 't,
    D: DrawTarget<Color = Rgb565>,
    TS: TimeSource + 't,
    TI: TaskInterface + 't,
>
{
    fn new() -> Self;
    fn process_midi(&mut self, _msg: &MidiMessage) {}
    fn process_ui_input<'u>(&'u mut self, msg: &'u UIInputEvent) -> Result<(), StdlibError>
    where
        't: 'u,
        <D as DrawTarget>::Error: Debug;

    fn render_screen(&mut self, screen: &mut D);
    fn update_output<
        T: for<'u> TryFrom<&'u NotePair, Error = E>,
        E: Debug,
        O: Deref<Target = impl Output<T, E>> + DerefMut,
    >(
        &self,
        mut _output: O,
    ) -> Result<(), E> {
        Ok(())
    }
    fn setup(&mut self);
    fn run(&mut self, program_time: u32, task_iface: &mut TI);
}
