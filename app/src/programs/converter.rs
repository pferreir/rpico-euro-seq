use core::{convert::TryInto, fmt::Debug};

use defmt::info;
use embedded_graphics::{
    draw_target::DrawTarget,
    image::Image,
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::{Point, Primitive, RgbColor, Size, WebColors},
    primitives::PrimitiveStyle,
    primitives::{Line, PrimitiveStyleBuilder, Rectangle},
    text::Text,
    Drawable,
};
use embedded_midi::MidiMessage;
use heapless::{spsc::Queue, String, Vec};
use tinybmp::{Bmp, ParseError};

use crate::{
    gate_cv::Output,
    ui::UIInputEvent,
    util::{midi_note_to_lib, QueuePoppingIter},
};
use ufmt::uwrite;
use voice_lib::{NotePair, VoiceState};

use super::Program;

const SCREEN_WIDTH: u32 = crate::screen::SCREEN_WIDTH as u32;

const NUM_VOICES: usize = 2;
const HISTORY_SIZE: usize = 1024;
const NOTE_HEIGHT: i32 = 4;
const NUM_VERTICAL_NOTES: i32 = 20;
const HEIGHT_ROLL: i32 = NOTE_HEIGHT * NUM_VERTICAL_NOTES;
const MS_PER_WIDTH: u32 = 10000;

static PLAY_ICON: &[u8] = include_bytes!("../../assets/play.bmp");
static PAUSE_ICON: &[u8] = include_bytes!("../../assets/pause.bmp");
static RECORD_ICON: &[u8] = include_bytes!("../../assets/record.bmp");
static RECORD_ON_ICON: &[u8] = include_bytes!("../../assets/record_on.bmp");
static STOP_ICON: &[u8] = include_bytes!("../../assets/stop.bmp");
static STOP_ON_ICON: &[u8] = include_bytes!("../../assets/stop_on.bmp");
static BEGINNING_ICON: &[u8] = include_bytes!("../../assets/beginning.bmp");
static SEEK_ICON: &[u8] = include_bytes!("../../assets/seek.bmp");

enum State {
    Stopped,
    Paused(/* at: */ u32),
    Playing(/* since_prog_time: */ u32, /* start_pos: */ u32),
    Recording(/* since_prog_time: */ u32, /* start_pos: */ u32),
}

impl State {
    fn get_time(&self, program_time: u32) -> u32 {
        match self {
            State::Stopped => 0,
            State::Paused(at) => *at,
            State::Playing(since, start_pos) | State::Recording(since, start_pos) => {
                start_pos + (program_time - since)
            }
        }
    }
}

#[derive(Copy, Clone)]
#[repr(u8)]
enum UIAction {
    PlayPause = 0,
    Stop = 1,
    Record = 2,
    Beginning = 3,
    Seek = 4,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum VoiceConfig {
    Mono(u8),
    PolySteal,
}

const NUM_UI_ACTIONS: usize = 5;

impl UIAction {
    fn button_pos(&self) -> Point {
        match self {
            UIAction::PlayPause => Point::new(0, 0),
            UIAction::Stop => Point::new(25, 0),
            UIAction::Record => Point::new(50, 0),
            UIAction::Beginning => Point::new(80, 0),
            UIAction::Seek => Point::new(105, 0),
        }
    }
}

impl From<u8> for UIAction {
    fn from(v: u8) -> Self {
        match v % NUM_UI_ACTIONS as u8 {
            0 => UIAction::PlayPause,
            1 => UIAction::Stop,
            2 => UIAction::Record,
            3 => UIAction::Beginning,
            4 => UIAction::Seek,
            _ => unreachable!(),
        }
    }
}

struct RecorderBox<'t> {
    recording_since: Option<u32>,
    voice_config: VoiceConfig,
    voice_state: VoiceState<'t, NUM_VOICES, HISTORY_SIZE>,
}

impl<'t> RecorderBox<'t> {
    fn new() -> Self {
        Self {
            recording_since: None,
            voice_state: VoiceState::new(),
            voice_config: VoiceConfig::Mono(0),
        }
    }

    fn key_pressed(&mut self, n: NotePair, t: u32) {
        match self.voice_config {
            VoiceConfig::PolySteal => self.voice_state.set_poly(n, t),
            VoiceConfig::Mono(v) => self.voice_state.set_mono(v, n, t),
        }
    }

    fn key_released(&mut self, n: NotePair, t: u32) {
        match self.voice_config {
            VoiceConfig::PolySteal => self.voice_state.clear_poly(n, t),
            VoiceConfig::Mono(v) => self.voice_state.clear_mono(v, t),
        }
    }

    fn get_voice_state(&self) -> [Option<&NotePair>; NUM_VOICES] {
        self.voice_state
            .iter_voices()
            .map(|x| x.as_ref())
            .collect::<Vec<Option<&NotePair>, NUM_VOICES>>()
            .as_slice()
            .try_into()
            .unwrap()
    }

    fn iter_notes_since(&self, t: u32) -> impl Iterator<Item = &(NotePair, u32, Option<u32>)> {
        self.voice_state.since(t)
    }

    fn start_recording(&mut self, program_time: u32) {
        self.recording_since = Some(program_time);
    }

    fn stop_recording(&mut self, program_time: u32) {
        self.recording_since = None;

        // Figure which voices to clear
        let to_clear = self
            .voice_state
            .iter_voices()
            .enumerate()
            .filter_map(|(n, v)| v.map(|_| n as u8))
            .collect::<Vec<u8, NUM_VOICES>>();

        for n in to_clear {
            self.voice_state.clear_mono(n as u8, program_time);
        }
    }
}

pub struct ConverterProgram<'t> {
    program_time: u32,
    current_note: i8,

    midi_queue: Queue<MidiMessage, 16>,
    recorder: RecorderBox<'t>,
    state: State,

    // UI
    selected_action: UIAction,
    // Icons
    play_icon: Bmp<'t, Rgb565>,
    pause_icon: Bmp<'t, Rgb565>,
    record_icon: Bmp<'t, Rgb565>,
    record_on_icon: Bmp<'t, Rgb565>,
    stop_icon: Bmp<'t, Rgb565>,
    stop_on_icon: Bmp<'t, Rgb565>,
    beginning_icon: Bmp<'t, Rgb565>,
    seek_icon: Bmp<'t, Rgb565>,
}

fn draw_notes<'t, D, I: IntoIterator<Item = &'t (NotePair, u32, Option<u32>)>>(
    top: i32,
    from_note: i8,
    start_time: u32,
    slots: I,
    screen: &mut D,
) where
    D: DrawTarget<Color = Rgb565>,
    <D as DrawTarget>::Error: Debug,
{
    let to_note = from_note.saturating_add(NUM_VERTICAL_NOTES as i8);

    let note_style = PrimitiveStyleBuilder::new()
        .fill_color(Rgb565::BLUE)
        .build();

    for (note, start, end) in slots {
        let note: i8 = note.into();
        if (note < from_note) || (note > to_note) {
            continue;
        }
        let start_x = if *start <= start_time {
            0
        } else {
            (SCREEN_WIDTH - 1) * (start - start_time) / MS_PER_WIDTH
        };
        let end_x = match end {
            Some(end_time) => (SCREEN_WIDTH - 1) * (end_time - start_time) / MS_PER_WIDTH,
            None => (SCREEN_WIDTH - 1),
        };
        let y = top + (NUM_VERTICAL_NOTES - 1 - (note - from_note) as i32) * NOTE_HEIGHT;

        if end_x > start_x {
            Rectangle::new(
                Point::new(start_x as i32, y),
                Size::new(end_x - start_x, NOTE_HEIGHT as u32),
            )
            .into_styled(note_style)
            .draw(screen)
            .unwrap();
        }
    }
}

fn draw_timeline<D>(top: i32, from: i8, screen: &mut D)
where
    D: DrawTarget<Color = Rgb565>,
    <D as DrawTarget>::Error: Debug,
{
    let frame_style = PrimitiveStyleBuilder::new()
        .stroke_color(Rgb565::WHITE)
        .stroke_width(1)
        .fill_color(Rgb565::BLACK)
        .build();

    let rect = Rectangle::new(
        Point::new(0, top),
        Size::new(SCREEN_WIDTH, HEIGHT_ROLL as u32),
    );
    rect.into_styled(frame_style).draw(screen).unwrap();

    let mark_style = PrimitiveStyleBuilder::new()
        .stroke_color(Rgb565::WHITE)
        .stroke_width(1)
        .build();
    let white_key_style = PrimitiveStyleBuilder::new()
        .fill_color(Rgb565::WHITE)
        .build();
    let line_style = PrimitiveStyleBuilder::new()
        .stroke_color(Rgb565::CSS_GRAY)
        .stroke_width(1)
        .build();

    for i in 0..20 {
        Line::new(
            Point::new(0, top + (NUM_VERTICAL_NOTES - 1 - i) * NOTE_HEIGHT),
            Point::new(6, top + (NUM_VERTICAL_NOTES - 1 - i) * NOTE_HEIGHT),
        )
        .into_styled(mark_style)
        .draw(screen)
        .unwrap();
        Line::new(
            Point::new(7, top + (NUM_VERTICAL_NOTES - 1 - i) * NOTE_HEIGHT),
            Point::new(
                (SCREEN_WIDTH - 2) as i32,
                top + (NUM_VERTICAL_NOTES - 1 - i) * NOTE_HEIGHT,
            ),
        )
        .into_styled(line_style)
        .draw(screen)
        .unwrap();

        let NotePair(note, _) = (from + i as i8).into();
        if !note.is_black_key() {
            Rectangle::new(
                Point::new(1, top + (NUM_VERTICAL_NOTES - 1 - i) * NOTE_HEIGHT),
                Size::new(5, NOTE_HEIGHT as u32),
            )
            .into_styled(white_key_style)
            .draw(screen)
            .unwrap();
        }
    }

    Line::new(Point::new(6, top), Point::new(6, top + HEIGHT_ROLL - 1))
        .into_styled(mark_style)
        .draw(screen)
        .unwrap();
}

impl<'t> ConverterProgram<'t> {
    fn draw_buttons<D>(&self, pos: Point, screen: &mut D)
    where
        D: DrawTarget<Color = Rgb565>,
        <D as DrawTarget>::Error: Debug,
    {
        let Point { x, y } = pos;
        Image::new(
            if let State::Playing(_, _) = self.state {
                &self.pause_icon
            } else {
                &self.play_icon
            },
            pos + UIAction::PlayPause.button_pos(),
        )
        .draw(screen)
        .unwrap();
        Image::new(
            if let State::Stopped = self.state {
                &self.stop_on_icon
            } else {
                &self.stop_icon
            },
            pos + UIAction::Stop.button_pos(),
        )
        .draw(screen)
        .unwrap();
        Image::new(
            if let State::Recording(_, _) = self.state {
                &self.record_on_icon
            } else {
                &self.record_icon
            },
            pos + UIAction::Record.button_pos(),
        )
        .draw(screen)
        .unwrap();
        Image::new(&self.beginning_icon, pos + UIAction::Beginning.button_pos())
            .draw(screen)
            .unwrap();
        Image::new(&self.seek_icon, pos + UIAction::Seek.button_pos())
            .draw(screen)
            .unwrap();

        Rectangle::new(pos + self.selected_action.button_pos(), Size::new(26, 16))
            .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 1))
            .draw(screen)
            .unwrap();
    }
}

impl<'t> Program for ConverterProgram<'t> {
    fn new() -> Self {
        Self {
            current_note: 72, // C5,

            midi_queue: Queue::new(),
            recorder: RecorderBox::new(),
            program_time: 0,
            state: State::Stopped,

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
        let current_time = self.state.get_time(self.program_time);
        screen.clear(Rgb565::CSS_DARK_SLATE_BLUE).unwrap();
        draw_timeline(0, self.current_note, screen);
        draw_notes(
            0,
            self.current_note,
            current_time.saturating_sub(MS_PER_WIDTH),
            self.recorder
                .iter_notes_since(current_time.saturating_sub(MS_PER_WIDTH)),
            screen,
        );

        self.draw_buttons(Point::new(10, 100), screen);

        let STYLE_CYAN: MonoTextStyle<Rgb565> = MonoTextStyle::new(&FONT_10X20, Rgb565::CYAN);
    }

    fn process_ui_input(&mut self, msg: &UIInputEvent) {
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
                let current_time = self.state.get_time(self.program_time);
                self.state = match self.selected_action {
                    UIAction::PlayPause => match self.state {
                        State::Playing(_, _) => State::Paused(current_time),
                        State::Stopped | State::Paused(_) | State::Recording(_, _) => {
                            State::Playing(self.program_time, current_time)
                        }
                    },
                    UIAction::Stop => {
                        if let State::Recording(_, _) = self.state {
                            self.recorder.stop_recording(self.program_time);
                        }
                        State::Stopped
                    }
                    UIAction::Record => {
                        self.recorder.start_recording(self.program_time);
                        State::Recording(self.program_time, current_time)
                    }
                    UIAction::Beginning => {
                        if let State::Recording(_, _) = self.state {
                            self.recorder.stop_recording(self.program_time);
                        }
                        State::Stopped
                    }
                    UIAction::Seek => todo!(),
                }
            }
            _ => {}
        }
    }

    fn process_midi(&mut self, msg: &MidiMessage) {
        self.midi_queue.enqueue(msg.clone()).unwrap();
    }

    fn update_output(&self, output: &mut impl Output) {
        let [v1, v2] = self.recorder.get_voice_state();

        match v1 {
            Some(n) => {
                output.set_ch0(n.into());
                output.set_gate0(true);
            }
            None => {
                output.set_gate0(false);
            }
        }

        match v2 {
            Some(n) => {
                output.set_ch1(n.into());
                output.set_gate1(true);
            }
            None => {
                output.set_gate1(false);
            }
        }
    }

    fn run(&mut self, program_time: u32) {
        self.program_time = program_time;
        for msg in QueuePoppingIter::new(&mut self.midi_queue) {
            match msg {
                MidiMessage::NoteOff(_, n, _) => {
                    self.recorder
                        .key_released(midi_note_to_lib(n), program_time);
                }
                MidiMessage::NoteOn(_, n, v) => {
                    if v == 0.into() {
                        // equivalent to NoteOff
                        self.recorder
                            .key_released(midi_note_to_lib(n), program_time);
                    } else {
                        self.recorder.key_pressed(midi_note_to_lib(n), program_time);
                    }
                }
                _ => {}
            }
        }
    }
}
