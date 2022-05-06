use core::{
    cmp::{max, min},
    fmt::Debug,
    include_bytes,
    marker::PhantomData,
};

use embedded_graphics::{
    draw_target::DrawTarget,
    image::Image,
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::{Point, Primitive, RgbColor, Size, WebColors},
    primitives::PrimitiveStyle,
    primitives::{Line, PrimitiveStyleBuilder, Rectangle},
    Drawable,
};
use embedded_midi::MidiMessage;
use heapless::{spsc::Queue, String, Vec};
use tinybmp::Bmp;

use crate::{
    log,
    ui::UIInputEvent,
    util::{midi_note_to_lib, GateOutput, QueuePoppingIter},
};
use ufmt::uwrite;
use voice_lib::{NoteFlag, NotePair, VoiceTrack};

use super::{Program};

const SCREEN_WIDTH: u32 = crate::screen::SCREEN_WIDTH as u32;

const NUM_VOICES: usize = 2;
const HISTORY_SIZE: usize = 1024;
const HISTORY_SIZE_DIV_4: usize = 256;
const NOTE_HEIGHT: i32 = 4;
const NUM_VERTICAL_NOTES: i32 = 20;
const NUM_HORIZONTAL_BEATS: u32 = 20;
const PIXELS_PER_BEAT: u32 = SCREEN_WIDTH / NUM_HORIZONTAL_BEATS;
const HEIGHT_ROLL: i32 = NOTE_HEIGHT * NUM_VERTICAL_NOTES;

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
    Paused(/* at_time: */ u32, /* at_beat: */ u32),
    Playing(/* time: */ u32, /* beat: */ u32),
    Recording(/* time: */ u32, /* beat: */ u32),
}

impl State {
    fn get_time(&self) -> (u32, u32) {
        match self {
            State::Stopped => (0, 0),
            State::Paused(time, beat) => (*time, *beat),
            State::Playing(time, beat) | State::Recording(time, beat) => (*time, *beat),
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

struct MonoRecorderBox<'t> {
    voice_state: VoiceTrack<HISTORY_SIZE, HISTORY_SIZE_DIV_4>,
    current_note: Vec<NotePair, NUM_VOICES>,
    keys_changed: bool,
    _t: &'t PhantomData<()>,
}

impl<'t> MonoRecorderBox<'t> {
    fn new() -> Self {
        Self {
            voice_state: VoiceTrack::new(),
            current_note: Vec::new(),
            keys_changed: false,
            _t: &PhantomData,
        }
    }

    fn key_pressed(&mut self, beat: usize, n: NotePair) {
        self.current_note.push(n).unwrap();
        self.voice_state.set_note(beat, |_| (n, NoteFlag::Note));
        self.keys_changed = true;
        let mut text = String::<32>::new();
        uwrite!(text, "KEY PRESS {}: {:?}", beat, n).unwrap();
        log::info(&text);

    }

    fn key_released(&mut self, beat: usize, n: NotePair) {
        self.current_note = self
            .current_note
            .iter()
            .filter(|e| *e != &n)
            .cloned()
            .collect();
        self.keys_changed = true;
    }

    fn beat(&mut self, beat: usize) {
        if !self.keys_changed && let Some(n) = self.current_note.last() {
            self.voice_state.set_note(beat, |_| (*n, NoteFlag::Legato));
        }

        // initialize already next note if there is at least a pressed one
        if let Some(n) = self.current_note.last() {
            self.voice_state.set_note(beat + 1, |_| (*n, NoteFlag::Legato));
        }
        self.keys_changed = false;
    }

    fn iter_notes_since(
        &'t self,
        t: usize,
        num: usize,
    ) -> impl Iterator<Item = (usize, NotePair, NoteFlag)> + 't {
        self.voice_state.since(t, num)
    }
}

pub struct SequencerProgram<'t> {
    current_note: u8,
    program_time: u32,
    prev_program_time: Option<u32>,

    midi_queue: Queue<MidiMessage, 16>,
    bpm: u16,
    recorder: MonoRecorderBox<'t>,
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

fn draw_timeline<D>(top: i32, from: u8, screen: &mut D)
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

        let NotePair(note, _) = (from + i as u8).into();
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

impl<'t> SequencerProgram<'t> {
    fn draw_buttons<D>(&self, pos: Point, screen: &mut D)
    where
        D: DrawTarget<Color = Rgb565>,
        <D as DrawTarget>::Error: Debug,
    {
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

    fn draw_notes<D, I: IntoIterator<Item = (usize, NotePair, NoteFlag)>>(
        &self,
        top: i32,
        from_note: u8,
        start_time: i32,
        slots: I,
        screen: &mut D,
    ) where
        D: DrawTarget<Color = Rgb565>,
        <D as DrawTarget>::Error: Debug,
    {
        let to_note = from_note.saturating_add(NUM_VERTICAL_NOTES as u8);

        let note_style = PrimitiveStyleBuilder::new()
            .fill_color(Rgb565::BLUE)
            .build();

        for (beat, note, flag) in slots.into_iter() {
            let note: u8 = (&note).into();
            if (note < from_note) || (note > to_note) {
                continue;
            }
            let beat_t = (beat as u32) * 60_000 / self.bpm as u32;
            let start_x =
                max(0, beat_t as i32 - start_time) as u32 * self.bpm as u32 * PIXELS_PER_BEAT
                    / 60_000;
            let next_beat_t = beat_t as i32 + 60_000 / self.bpm as i32;
            let end_x = min(
                SCREEN_WIDTH - 1,
                (next_beat_t - start_time) as u32 * self.bpm as u32 * PIXELS_PER_BEAT / 60_000,
            );

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
}

impl<'t> Program for SequencerProgram<'t> {
    fn new() -> Self {
        Self {
            current_note: 72, // C5,
            prev_program_time: None,
            program_time: 0,
            bpm: 100,
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
        let (current_time, beat) = self.state.get_time();
        screen.clear(Rgb565::CSS_DARK_SLATE_BLUE).unwrap();
        draw_timeline(0, self.current_note, screen);

        self.draw_notes(
            0,
            self.current_note,
            current_time as i32 - (NUM_HORIZONTAL_BEATS as i32 * 60_000 / (self.bpm as i32)),
            self.recorder.iter_notes_since(
                beat.saturating_sub(NUM_HORIZONTAL_BEATS) as usize,
                NUM_HORIZONTAL_BEATS as usize + 1,
            ),
            screen,
        );

        self.draw_buttons(Point::new(10, 100), screen);
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

    fn update_output<'u, 'v, T: From<&'u NotePair>>(&'v self, output: &mut impl GateOutput<'u, T>) where
        'v: 'u
    {
        // TODO: polyphonic
        match self.recorder.current_note.last() {
            None => {
                output.set_gate0(false);
            },
            Some(np) => {
                output.set_gate0(true);
                output.set_ch0(np.into());
            }
        }
    }

    fn run(&mut self, program_time: u32) {
        self.program_time = program_time;

        // let mut t: String<32> = String::new();
        // uwrite!(t, "{}", program_time).unwrap();
        // self.logger().info(&t);

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
