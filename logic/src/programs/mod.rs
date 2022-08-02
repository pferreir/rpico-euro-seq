mod debug;
mod sequencer;

use alloc::{boxed::Box, collections::BTreeMap, vec::Vec};
use core::{
    fmt::Debug,
    future::Future,
    ops::{Deref, DerefMut},
    pin::Pin,
};
pub use debug::DebugProgram;
use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb565, prelude::WebColors};
use embedded_midi::MidiMessage;
use embedded_sdmmc::{BlockDevice, TimeSource};
pub use sequencer::SequencerProgram;
use voice_lib::NotePair;

use crate::{
    stdlib::{
        CVChannel, FileSystem, GateChannel, SignalId, StdlibError, Task, TaskInterface,
        TaskManager, TaskResult, Output,
    },
    ui::UIInputEvent,
};

#[derive(Debug)]
pub enum ProgramError<D: BlockDevice> {
    Stdlib(StdlibError<D>),
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
    fn process_midi(&mut self, msg: &MidiMessage) {}
    fn process_ui_input<'u>(&'u mut self, msg: &'u UIInputEvent) -> Result<(), ProgramError<B>>
    where
        't: 'u,
        <D as DrawTarget>::Error: Debug;

    fn render_screen(&mut self, screen: &mut D);
    fn update_output<T: for<'u> TryFrom<&'u NotePair, Error = E>, E: Debug, O: Deref<Target = impl Output<T, E>> + DerefMut>(
        &self,
        mut _output: O,
    ) -> Result<(), E> {
        Ok(())
    }
    fn setup(&mut self);
    fn run(&mut self, program_time: u32, task_iface: &mut TI);
}
