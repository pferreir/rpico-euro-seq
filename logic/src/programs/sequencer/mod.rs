use core::{fmt::Debug, future::Future};

use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use embedded_midi::MidiMessage;
use embedded_sdmmc::{TimeSource, BlockDevice};
use heapless::spsc::Queue;
use tinybmp::Bmp;

use self::{
    config::Config,
    recorder::MonoRecorderBox,
    ui::{icons::*, UIAction, NUM_UI_ACTIONS},
};
use crate::{
    stdlib::FileSystem,
    ui::UIInputEvent,
    util::{midi_note_to_lib, GateOutput, QueuePoppingIter},
};
use voice_lib::NotePair;

use super::{Program, ProgramError};

mod config;
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

#[derive(Copy, Clone, PartialEq, Eq)]
enum VoiceConfig {
    Mono(u8),
    PolySteal,
}

pub struct SequencerProgram<'t, B: BlockDevice, TS: TimeSource> {
    fs: FileSystem<B, TS>,
    pub(crate) current_note: u8,
    program_time: u32,
    prev_program_time: Option<u32>,

    midi_queue: Queue<MidiMessage, 16>,
    pub(crate) bpm: u16,
    pub(crate) recorder: MonoRecorderBox<'t>,
    pub(crate) state: State,

    // UI
    pub(crate) selected_action: UIAction,
    // Icons
    pub(crate) play_icon: Bmp<'t, Rgb565>,
    pub(crate) pause_icon: Bmp<'t, Rgb565>,
    pub(crate) record_icon: Bmp<'t, Rgb565>,
    pub(crate) record_on_icon: Bmp<'t, Rgb565>,
    pub(crate) stop_icon: Bmp<'t, Rgb565>,
    pub(crate) stop_on_icon: Bmp<'t, Rgb565>,
    pub(crate) beginning_icon: Bmp<'t, Rgb565>,
    pub(crate) seek_icon: Bmp<'t, Rgb565>,
}

impl<'t, B: BlockDevice, TS: TimeSource> Program<B, TS> for SequencerProgram<'t, B, TS> {
    type SetupFuture<'a> = impl Future<Output = Result<(), ProgramError<B>>> + 'a where Self: 'a;

    fn new(fs: FileSystem<B, TS>) -> Self {
        Self {
            fs,
            current_note: 72, // C5,
            prev_program_time: None,
            program_time: 0,
            bpm: 50,
            midi_queue: Queue::new(),
            recorder: MonoRecorderBox::new(),
            state: State::Recording(0, 0),

            // UI
            selected_action: UIAction::PlayPause,
            // Icons
            play_icon: Bmp::from_slice(PLAY_ICON).unwrap(),
            pause_icon: Bmp::from_slice(PAUSE_ICON).unwrap(),
            record_icon: Bmp::from_slice(RECORD_ICON).unwrap(),
            record_on_icon: Bmp::from_slice(RECORD_ON_ICON).unwrap(),
            stop_icon: Bmp::from_slice(STOP_ICON).unwrap(),
            stop_on_icon: Bmp::from_slice(STOP_ON_ICON).unwrap(),
            beginning_icon: Bmp::from_slice(BEGINNING_ICON).unwrap(),
            seek_icon: Bmp::from_slice(SEEK_ICON).unwrap(),
        }
    }

    fn render_screen<D>(&self, screen: &mut D)
    where
        D: DrawTarget<Color = Rgb565>,
        <D as DrawTarget>::Error: Debug,
    {
        self._render_screen(screen);
    }

    fn process_ui_input(&mut self, msg: &UIInputEvent) {
        let (state_time, state_beat) = self.state.get_time();
        match msg {
            UIInputEvent::EncoderTurn(v) => {
                // let new_current_note = (self.current_note as i8) + v;
                // self.current_note = if new_current_note < 0 {
                //     0
                // } else {
                //     new_current_note as i8
                // }
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
    }

    fn process_midi(&mut self, msg: &MidiMessage) {
        self.midi_queue.enqueue(msg.clone()).unwrap();
    }

    fn update_output<'u, 'v, T: From<&'u NotePair>>(&'v self, output: &mut impl GateOutput<'u, T>)
    where
        'v: 'u,
    {
        // TODO: polyphonic
        match self.recorder.last_note() {
            None => {
                output.set_gate0(false);
            }
            Some(np) => {
                output.set_gate0(true);
                output.set_ch0(np.into());
            }
        }
    }

    fn setup<'a>(&'a mut self) -> Self::SetupFuture<'a> {
        async {
            Config::load(&mut self.fs).await.map_err(ProgramError::Stdlib)?;
            Ok(())
        }
    }

    fn run(&mut self, program_time: u32) {
        self.program_time = program_time;

        let time_diff = match self.prev_program_time {
            Some(t) => self.program_time - t,
            None => 0u32,
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
    }
}
