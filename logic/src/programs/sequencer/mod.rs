use core::{
    fmt::Debug, marker::PhantomData, ops::{DerefMut, Deref},
};

use alloc::{boxed::Box, format, vec::Vec};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use embedded_midi::{MidiMessage};
use embedded_sdmmc::{BlockDevice, TimeSource};
use heapless::{spsc::Queue, String};

use self::{
    recorder::MonoRecorderBox,
    ui::{
        actions::{UIAction, NUM_UI_ACTIONS},
        overlays::FileMenu
    }, config::Config,
};
use crate::{
    log::info,
    stdlib::{
        ui::{Overlay, OverlayResult, UIInputEvent},
        StdlibError,
        TaskInterface, TaskType, Output, GateChannelId, CVChannelId, FileType,
    },
    util::{midi_note_to_lib, DiscreetUnwrap, QueuePoppingIter},
};
use voice_lib::{Note, NoteFlag, NotePair};

use super::{Program, ProgramError};

mod config;
mod data;
mod recorder;
mod ui;


pub(crate) enum State {
    Stopped,
    Paused(/* at_time: */ u32, /* at_beat: */ u32),
    Playing(/* time: */ u32, /* beat: */ u32),
    Recording(/* time: */ u32, /* beat: */ u32),
}

impl State {
    pub(crate) fn get_time(&self) -> (u32, u32) {
        match self {
            State::Stopped => (0, 0),
            State::Paused(time, beat) => (*time, *beat),
            State::Playing(time, beat) | State::Recording(time, beat) => (*time, *beat),
        }
    }
}

pub struct OverlayManager<
    't,
    B: BlockDevice,
    TS: TimeSource,
    D: DrawTarget<Color = Rgb565>,
    TI: TaskInterface + 't,
> where
    D::Error: Debug,
{
    pub(crate) stack:
        Option<Vec<Box<dyn Overlay<'t, D, SequencerProgram<'t, B, TS, D, TI>, B, TS, TI> + 't>>>,
    pub(crate) pending_ops:
        Vec<OverlayResult<'t, D, SequencerProgram<'t, B, TS, D, TI>, B, TS, TI>>,
}

impl<'t, B: BlockDevice, TS: TimeSource, D: DrawTarget<Color = Rgb565>, TI: TaskInterface>
    OverlayManager<'t, B, TS, D, TI>
where
    D::Error: Debug,
{
    fn new() -> Self {
        let mut stack: Vec<Box<dyn Overlay<'t, D, SequencerProgram<'t, B, TS, D, TI>, B, TS, TI>>> =
            Vec::new();
        stack.push(Box::new(FileMenu::default()));
        Self {
            stack: Some(stack),
            pending_ops: Vec::new(),
        }
    }

    pub(crate) fn process_input(&mut self, msg: &UIInputEvent) -> Result<bool, ProgramError> {
        let mut overlays = self.stack.take().unwrap();
        let res = match overlays.last_mut() {
            Some(o) => {
                self.pending_ops.push(o.process_ui_input(msg));
                true
            }
            None => false,
        };
        self.stack.replace(overlays);
        Ok(res)
    }

    pub(crate) fn draw(&mut self, screen: &mut D) {
        let mut overlays = self.stack.take().unwrap();
        for overlay in overlays.iter_mut() {
            overlay.draw(screen).duwrp();
        }
        self.stack.replace(overlays);
    }

    pub(crate) fn run(
        &mut self,
        program: &mut SequencerProgram<'t, B, TS, D, TI>,
        task_iface: &mut TI,
    ) {
        let mut overlays = self.stack.take().unwrap();

        for overlay in overlays.iter_mut() {
            match overlay.run().duwrp() {
                Some(f) => f(program, task_iface).unwrap(),
                None => {}
            }
        }

        for operation in self.pending_ops.drain(0..(self.pending_ops.len())) {
            match operation {
                OverlayResult::Nop => {}
                OverlayResult::Push(o) => {
                    overlays.push(o);
                }
                OverlayResult::Replace(o) => {
                    overlays.push(o);
                }
                OverlayResult::Close => {
                    overlays.pop();
                }
                OverlayResult::CloseOnSignal(_) => {}
            }
        }

        self.stack.replace(overlays);
    }
}

pub struct SequencerProgram<
    't,
    B: BlockDevice,
    TS: TimeSource,
    D: DrawTarget<Color = Rgb565>,
    TI: TaskInterface,
> where
    D: 't,
    <D as DrawTarget>::Error: Debug,
{
    pub(crate) current_note: u8,
    program_time: u32,
    prev_program_time: Option<u32>,

    midi_queue: Queue<MidiMessage, 16>,
    pub(crate) bpm: u16,
    pub(crate) recorder: MonoRecorderBox<'t>,
    pub(crate) state: State,

    // UI
    pub(crate) selected_action: UIAction,
    pub(crate) overlay_manager: Option<OverlayManager<'t, B, TS, D, TI>>,

    _d: PhantomData<D>,
}

impl<'t, B: BlockDevice, TS: TimeSource, D: DrawTarget<Color = Rgb565>, TI: TaskInterface>
    SequencerProgram<'t, B, TS, D, TI>
where
    <D as DrawTarget>::Error: Debug,
{
    fn save(&mut self, file_name: String<12>) -> Result<TaskType, StdlibError> {
        self.recorder.set_file_name(&file_name);
        self.recorder.save_file()
    }

    fn _check_task_returns(&mut self, task_iface: &mut impl TaskInterface) {
        while let Ok(Some((id, result))) = task_iface.pop() {
            info(&format!("Task {} result: {:?}", id, result));
        }
    }

    fn _first_run(&mut self, task_iface: &mut TI) {
        task_iface.submit(TaskType::FileLoad(FileType::Config, "sequencer".into()));
    }
}

impl<
        't,
        B: BlockDevice + 't,
        TS: TimeSource + 't,
        D: DrawTarget<Color = Rgb565> + 't,
        TI: TaskInterface + 't,
    > Program<'t, B, D, TS, TI> for SequencerProgram<'t, B, TS, D, TI>
where
    <D as DrawTarget>::Error: Debug,
{
    fn new() -> Self {
        Self {
            current_note: 70, // C5,
            prev_program_time: None,
            program_time: 0,
            bpm: 50,
            midi_queue: Queue::new(),
            recorder: MonoRecorderBox::new(),
            state: State::Stopped,

            // UI
            selected_action: UIAction::PlayPause,
            overlay_manager: Some(OverlayManager::new()),
            // Icons
            _d: PhantomData,
        }
    }

    fn render_screen(&mut self, mut screen: &mut D) {
        self._render_screen(screen.deref_mut());
        let mut overlay_manager = self.overlay_manager.take().unwrap();
        overlay_manager.draw(screen.deref_mut());
        self.overlay_manager.replace(overlay_manager);
    }

    fn process_ui_input<'u>(&'u mut self, msg: &'u UIInputEvent) -> Result<(), ProgramError>
    where
        't: 'u,
    {
        let (state_time, state_beat) = self.state.get_time();

        let mut overlay_manager = self.overlay_manager.take().unwrap();

        let stop_here = overlay_manager.process_input(msg)?;

        self.overlay_manager.replace(overlay_manager);

        if stop_here {
            return Ok(());
        }

        match msg {
            UIInputEvent::EncoderTurn(v) => {
                self.selected_action = ((self.selected_action as i8)
                    .wrapping_add(*v)
                    .rem_euclid(NUM_UI_ACTIONS as i8)
                    as u8)
                    .into();
            }
            UIInputEvent::EncoderSwitch(true) => {
                self.state = match self.selected_action {
                    UIAction::PlayPause => match self.state {
                        State::Playing(_, _) => State::Paused(state_time, state_beat),
                        State::Paused(time, beat) => State::Playing(time, beat),
                        State::Stopped | State::Recording(_, _) => State::Playing(0, 0),
                    },
                    UIAction::Stop => State::Stopped,
                    UIAction::Record => State::Recording(state_time, state_beat),
                    UIAction::Beginning => State::Stopped,
                    UIAction::Seek => todo!(),
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn process_midi(&mut self, msg: &MidiMessage) {
        self.midi_queue.enqueue(msg.clone()).unwrap();
    }

    fn update_output<T: for<'u> TryFrom<&'u NotePair, Error = E>, E: Debug, O: Deref<Target = impl Output<T, E>> + DerefMut>(
        &self,
        mut output: O,
    ) -> Result<(), E> {
        // TODO: polyphonic
        match self.recorder.last_note() {
            None => {
                output.set_gate(GateChannelId::Gate0, false);
            }
            Some(np) => {
                output.set_gate(GateChannelId::Gate0, true);
                output.set_cv(CVChannelId::CV0, np.try_into()?);
            }
        }
        Ok(())
    }

    fn setup(&mut self) {
        // TODO: remove
        self.recorder
            .voice_state
            .set_note(0, (Some(NotePair(Note::C, 5)), NoteFlag::Note))
            .duwrp();
        self.recorder
            .voice_state
            .set_note(1, (Some(NotePair(Note::Eb, 5)), NoteFlag::Note))
            .duwrp();
        self.recorder
            .voice_state
            .set_note(2, (Some(NotePair(Note::G, 5)), NoteFlag::Note))
            .duwrp();
        self.recorder
            .voice_state
            .set_note(3, (Some(NotePair(Note::B, 5)), NoteFlag::Note))
            .duwrp();
        self.recorder
            .voice_state
            .set_note(4, (Some(NotePair(Note::G, 5)), NoteFlag::Note))
            .duwrp();
        self.recorder
            .voice_state
            .set_note(5, (Some(NotePair(Note::Eb, 5)), NoteFlag::Note))
            .duwrp();
        self.recorder
            .voice_state
            .set_note(6, (Some(NotePair(Note::C, 5)), NoteFlag::Note))
            .duwrp();
    }

    fn run(&mut self, program_time: u32, task_iface: &mut TI) {
        self.program_time = program_time;

        let time_diff = match self.prev_program_time {
            Some(t) => self.program_time - t,
            None => {
                self._first_run(task_iface);
                0u32
            },
        };

        match self.state {
            State::Recording(time, beat) => {
                let new_time = time + time_diff;
                let new_beat = new_time * self.bpm as u32 / 60_000;
                self.state = State::Recording(new_time, new_beat);

                if beat != new_beat {
                    self.recorder.beat(beat as usize);
                }
            }
            State::Playing(time, _) => {
                let new_time = time + time_diff;
                let new_beat = new_time * self.bpm as u32 / 60_000;
                self.state = State::Playing(new_time, new_beat);
            }
            _ => {}
        }

        self.prev_program_time = Some(self.program_time);

        let (_, beats) = self.state.get_time();

        for msg in QueuePoppingIter::new(&mut self.midi_queue) {
            match msg {
                MidiMessage::NoteOff(_, n, _) => {
                    self.recorder
                        .key_released(beats as usize, midi_note_to_lib(n));
                }
                MidiMessage::NoteOn(_, n, v) => {
                    if v == 0.into() {
                        // equivalent to NoteOff
                        self.recorder
                            .key_released(beats as usize, midi_note_to_lib(n));
                    } else {
                        self.recorder
                            .key_pressed(beats as usize, midi_note_to_lib(n));
                    }
                }
                _ => {}
            }
        }

        self._check_task_returns(task_iface);

        let mut overlay_manager = self.overlay_manager.take().unwrap();
        overlay_manager.run(self, task_iface);
        self.overlay_manager.replace(overlay_manager);
    }
}
