use core::{
    fmt::Debug, marker::PhantomData, ops::{DerefMut, Deref},
};

use alloc::{format, boxed::Box};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use embedded_midi::{MidiMessage};
use embedded_sdmmc::{BlockDevice, TimeSource};
use heapless::{spsc::Queue, String};

use self::{
    recorder::MonoRecorderBox,
    ui::{
        actions::{UIAction, NUM_UI_ACTIONS},
    }, config::Config
};
use crate::{
    log::{info, error, warning},
    stdlib::{
        ui::{UIInputEvent, OverlayManager},
        StdlibError,
        TaskInterface, TaskType, Output, GateChannelId, CVChannelId, TaskResult, FSError, FileContent,
    },
    util::{midi_note_to_lib, DiscreetUnwrap, QueuePoppingIter},
};
use voice_lib::{Note, NoteFlag, NotePair};

use super::Program;

mod config;
mod data;
mod recorder;
mod ui;


#[derive(Debug, PartialEq)]
pub(crate) enum State {
    Loading,
    Stopped,
    Paused(/* at_time: */ u32, /* at_beat: */ u32),
    Playing(/* time: */ u32, /* beat: */ u32),
    Recording(/* time: */ u32, /* beat: */ u32),
}

impl State {
    pub(crate) fn get_time(&self) -> (u32, u32) {
        match self {
            State::Loading => (0, 0),
            State::Stopped => (0, 0),
            State::Paused(time, beat) => (*time, *beat),
            State::Playing(time, beat) | State::Recording(time, beat) => (*time, *beat),
        }
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
    pub(crate) overlay_manager: Option<OverlayManager<'t, Self, B, TS, D, TI>>,

    _d: PhantomData<D>,
}

impl<'t, B: BlockDevice, TS: TimeSource, D: DrawTarget<Color = Rgb565>, TI: TaskInterface>
    SequencerProgram<'t, B, TS, D, TI>
where
    <D as DrawTarget>::Error: Debug,
{
    fn save(&mut self, file_name: String<8>) -> Result<TaskType, StdlibError> {
        self.recorder.set_file_name(&file_name);
        self.recorder.save_file()
    }

    fn _first_run(&mut self, task_iface: &mut TI) {
        task_iface.submit(TaskType::FileLoad("cfg".into(), "config.cbr".into())).unwrap();
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
            state: State::Loading,

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

    fn process_ui_input<'u>(&'u mut self, msg: &'u UIInputEvent) -> Result<(), StdlibError>
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
                        State::Loading | State::Stopped | State::Recording(_, _) => State::Playing(0, 0),
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

        // Process tasks
        while let Ok(Some((id, result))) = task_iface.pop() {
            // TODO: propagate until dialog
            info(&format!("Task {} result: {:?}", id, result));

            if self.state == State::Loading {
                match result {
                    TaskResult::FileContent(content) => {
                        info(&format!("Config loaded: {:?}", content));
                        // set state to stopped
                        self.state = State::Stopped;
                    },
                    TaskResult::Error(StdlibError::FS(FSError::FileNotFound)) => {
                        warning("Config file doesn't exist. Creating one.");
                        let task = TaskType::FileSave("cfg".into(), "config.cbr".into(), Box::new(Config::default()));
                        task_iface.submit(task).expect("Unable to submit task");
                    },
                    TaskResult::Done => {
                        warning("Done. Trying to read again...");
                        // done creating config file. read again.
                        let task = TaskType::FileLoad("cfg".into(), "config.cbr".into());
                        task_iface.submit(task).expect("Unable to submit task");
                    }
                    res => {
                        error(&format!("Completely unexpected task result: {:?}", res));
                    }
                }
            }
        }
    
        let mut overlay_manager = self.overlay_manager.take().unwrap();
        overlay_manager.run(self, task_iface).unwrap();
        self.overlay_manager.replace(overlay_manager);
    }
}
