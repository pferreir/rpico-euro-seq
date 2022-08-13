#![feature(generic_associated_types)]
#![feature(type_alias_impl_trait)]

extern crate alloc;

use core::future::Future;
use core::str;
use std::cell::RefCell;
use std::fmt::Debug;
use std::rc::Rc;

use embedded_graphics_web_simulator::{
    display::WebSimulatorDisplay, output_settings::OutputSettingsBuilder,
};
use embedded_sdmmc::TimeSource;
use embedded_sdmmc::{Block, BlockCount, BlockDevice, BlockIdx, Timestamp};
use futures::channel::mpsc::{self, Receiver, Sender};
use js_sys::{Date, Uint8Array};
use logic::log::info;
use logic::stdlib::ui::UIInputEvent;
use logic::stdlib::{
    CVChannel, CVChannelId, Channel, FileSystem, GateChannel, GateChannelId, Output, Task, TaskId,
    TaskInterface, TaskManager, TaskReturn, TaskType,
};
use midi_types::MidiMessage;
use serde::{Deserialize, Serialize};
use ufmt::{uDebug, uWrite};
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::spawn_local;
use web_sys::console;
use web_sys::AudioContext;
use web_sys::OscillatorNode;
use web_sys::Window;
use web_sys::{GainNode, OscillatorType};

use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb565, prelude::*};
use logic::LogLevel;
use logic::{
    programs::{Program, SequencerProgram},
    screen::{SCREEN_HEIGHT, SCREEN_WIDTH},
};
use voice_lib::{InvalidNotePair, NotePair};

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

thread_local! {
    static INPUT_QUEUE: RefCell<Vec<UIInputEvent>> = RefCell::new(Vec::new());
    static MIDI_QUEUE: RefCell<Vec<MidiMessage>> = RefCell::new(Vec::new());
}

#[inline(never)]
#[no_mangle]
unsafe fn _log(text: *const str, level: LogLevel) {
    let text = text.as_ref().unwrap();
    match level {
        LogLevel::Debug => console::debug_1(&text.into()),
        LogLevel::Info => console::info_1(&text.into()),
        LogLevel::Warning => console::warn_1(&text.into()),
        LogLevel::Error => console::error_1(&text.into()),
    }
}
struct MidiMsgWrapper(MidiMessage);

impl<'t> Deserialize<'t> for MidiMsgWrapper {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'t>,
    {
        let (on, c, k, v) = <(bool, u8, u8, u8)>::deserialize(deserializer).unwrap();
        Ok(MidiMsgWrapper(match on {
            true => MidiMessage::NoteOn(c.into(), k.into(), v.into()),
            false => MidiMessage::NoteOff(c.into(), k.into(), v.into()),
        }))
    }
}

impl Serialize for MidiMsgWrapper {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let msg = self.0.clone();
        match msg {
            MidiMessage::NoteOn(c, n, v) => {
                let t: (bool, u8, u8, u8) = (true, c.into(), n.into(), v.into());
                t.serialize(serializer)
            }
            MidiMessage::NoteOff(c, n, v) => {
                let t: (bool, u8, u8, u8) = (false, c.into(), n.into(), v.into());
                t.serialize(serializer)
            }
            _ => {
                unimplemented!()
            }
        }
    }
}

struct Frequency(f32);

impl TryFrom<&NotePair> for Frequency {
    type Error = InvalidNotePair;

    fn try_from(np: &NotePair) -> Result<Self, Self::Error> {
        let n: u8 = np.try_into()?;
        Ok(Self(440.0 * 2f32.powf((n as f32 - 69.0) / 12.0)))
    }
}

struct BrowserGateChannel {
    vol0: GainNode,
}

struct BrowserCVChannel {
    osc0: OscillatorNode,
}

impl GateChannel for BrowserGateChannel {}

impl Channel<bool> for BrowserGateChannel {
    fn set(&mut self, val: bool) {
        let g = self.vol0.gain();
        if val {
            g.set_value(1.0);
        } else {
            g.set_value(0.0);
        }
    }
}

impl CVChannel<Frequency> for BrowserCVChannel {
    type Error = <Frequency as TryFrom<&'static NotePair>>::Error;

    fn set_from_note(&mut self, val: &NotePair) -> Result<(), Self::Error> {
        self.set(val.try_into()?);
        Ok(())
    }
}

impl Channel<Frequency> for BrowserCVChannel {
    fn set(&mut self, val: Frequency) {
        self.osc0.frequency().set_value(val.0);
    }
}

struct BrowserOutput {
    gate0: BrowserGateChannel,
    cv0: BrowserCVChannel,
}

impl Output<Frequency, InvalidNotePair> for BrowserOutput {
    fn set_gate(&mut self, id: GateChannelId, value: bool) {
        match id {
            GateChannelId::Gate0 => {
                self.gate0.set(value);
            }
            GateChannelId::Gate1 => todo!(),
        }
    }

    fn set_cv(&mut self, id: logic::stdlib::CVChannelId, value: Frequency) {
        match id {
            CVChannelId::CV0 => {
                self.cv0.set(value);
            }
            CVChannelId::CV1 => todo!(),
        }
    }
}

impl BrowserOutput {
    fn new() -> Self {
        let ac = AudioContext::new().unwrap();
        let osc0 = ac.create_oscillator().unwrap();
        osc0.set_type(OscillatorType::Sawtooth);
        let vol0 = GainNode::new(&ac).unwrap();
        osc0.connect_with_audio_node(&vol0).unwrap();
        vol0.connect_with_audio_node(&ac.destination()).unwrap();
        osc0.start().unwrap();
        Self {
            gate0: BrowserGateChannel { vol0 },
            cv0: BrowserCVChannel { osc0 },
        }
    }
}

#[derive(Debug)]
enum LocalStorageDeviceError {
    OutOfRange,
    JS(JsValue),
}

impl uDebug for LocalStorageDeviceError {
    fn fmt<W>(&self, formatter: &mut ufmt::Formatter<W>) -> Result<(), W::Error>
    where
        W: uWrite + ?Sized,
    {
        let text = match self {
            LocalStorageDeviceError::OutOfRange => "Request out of range".to_owned(),
            LocalStorageDeviceError::JS(e) => format!("JS error: {:?}", e),
        };
        formatter.write_str(&text)?;
        Ok(())
    }
}

const LOCAL_STORAGE_SIZE: usize = 10 * 1024 * 1024;

#[derive(Debug)]
struct LocalStorageDevice;

#[wasm_bindgen(module = "/js/disk.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    async unsafe fn readFromDisk(start: u32, end: u32) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch)]
    async unsafe fn writeToDisk(start_idx: u32, data: Uint8Array) -> Result<JsValue, JsValue>;
}

impl BlockDevice for LocalStorageDevice {
    type Error = LocalStorageDeviceError;
    type ReadFuture<'b> = impl Future<Output = Result<(), Self::Error>> + 'b;
    type WriteFuture<'b> = impl Future<Output = Result<(), Self::Error>> + 'b;
    type BlocksFuture<'b> = impl Future<Output = Result<BlockCount, Self::Error>> + 'b;

    fn read<'a>(
        &'a mut self,
        blocks: &'a mut [Block],
        BlockIdx(start_block_idx): BlockIdx,
        _reason: &str,
    ) -> Self::ReadFuture<'a> {
        let start = start_block_idx * 512;
        let end = start_block_idx * 512 + blocks.len() as u32 * 512;

        async move {
            let val: Uint8Array = readFromDisk(start, end).await.unwrap().into();

            for (n, b) in blocks.iter_mut().enumerate() {
                let arr = val.subarray(n as u32 * Block::LEN_U32, (n as u32 + 1) * Block::LEN_U32);
                let mut block = Block::new();
                arr.copy_to(&mut block.contents);
                *b = block;
            }
            Ok(())
        }
    }

    fn write<'a>(
        &'a mut self,
        blocks: &'a [Block],
        BlockIdx(start_block_idx): BlockIdx,
    ) -> Self::WriteFuture<'a> {
        async move {
            let arr = Uint8Array::new_with_length(blocks.len() as u32 * 512);
            for (n, block) in blocks.iter().enumerate() {
                arr.subarray((n as u32) * 512, (n as u32 + 1) * 512)
                    .copy_from(&block.contents)
            }
            writeToDisk(start_block_idx * 512, arr).await.unwrap();
            Ok(())
        }
    }

    fn num_blocks(&self) -> Self::BlocksFuture<'_> {
        async move { Ok(BlockCount(LOCAL_STORAGE_SIZE as u32 / 512)) }
    }
}

pub struct WebTaskInterface {
    receiver: Receiver<TaskReturn>,
    sender: Sender<Task>,
    id_counter: u32,
}

impl WebTaskInterface {
    pub fn new(receiver: Receiver<TaskReturn>, sender: Sender<Task>) -> Self {
        Self {
            receiver,
            sender,
            id_counter: 0,
        }
    }
}

impl TaskInterface for WebTaskInterface {
    type Error = mpsc::TrySendError<Task>;

    fn submit(&mut self, task_type: TaskType) -> Result<TaskId, Self::Error> {
        let id = self.id_counter;
        self.id_counter = self.id_counter.wrapping_add(1);
        self.sender.try_send(Task(id, task_type)).map(|_| id)
    }

    fn pop(&mut self) -> Result<Option<TaskReturn>, Self::Error> {
        if let Ok(Some(ret)) = self.receiver.try_next() {
            Ok(Some(ret))
        } else {
            Ok(None)
        }
    }
}

struct JSTime;

impl TimeSource for JSTime {
    fn get_timestamp(&self) -> Timestamp {
        let d = Date::new_0();
        Timestamp {
            year_since_1970: (d.get_utc_full_year() - 1970) as u8,
            zero_indexed_month: d.get_utc_month() as u8,
            zero_indexed_day: (d.get_utc_date() - 1) as u8,
            hours: d.get_utc_hours() as u8,
            minutes: d.get_utc_minutes() as u8,
            seconds: d.get_utc_seconds() as u8,
        }
    }
}

fn window() -> web_sys::Window {
    web_sys::window().expect("no global `window` exists")
}

fn request_animation_frame(f: &Closure<dyn FnMut()>) {
    window()
        .request_animation_frame(f.as_ref().unchecked_ref())
        .expect("should register `requestAnimationFrame` OK");
}

fn loop_func<
    't,
    P: Program<'t, LocalStorageDevice, WebSimulatorDisplay<Rgb565>, JSTime, WebTaskInterface>
        + 'static,
>(
    mut program: P,
    output: BrowserOutput,
    window: Window,
    mut display: WebSimulatorDisplay<Rgb565>,
    rx_channel: Receiver<TaskReturn>,
    tx_channel: Sender<Task>,
) {
    let mut task_iface = WebTaskInterface::new(rx_channel, tx_channel);
    let output = Rc::new(RefCell::new(output));
    let f = Rc::new(RefCell::new(None));
    let g = f.clone();

    *(g.as_ref()).borrow_mut() = Some(Closure::wrap(Box::new(move || {
        {
            MIDI_QUEUE.with(|vec| {
                for msg in vec.borrow().iter() {
                    program.process_midi(msg);
                }
                vec.borrow_mut().clear();
            });

            INPUT_QUEUE.with(|vec| {
                for msg in vec.borrow().iter() {
                    program.process_ui_input(msg).unwrap();
                }
                vec.borrow_mut().clear();
            });

            let now = window
                .performance()
                .expect("should have a Performance")
                .now();

            program.run(now.floor() as u32, &mut task_iface);
        }

        {
            display.clear(Rgb565::BLACK).unwrap();
            program.render_screen(&mut display);
            display.flush().expect("could not flush buffer");
            program.update_output(output.borrow_mut()).unwrap();
        }
        // Schedule ourself for another requestAnimationFrame callback.
        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut()>));

    request_animation_frame(g.borrow().as_ref().unwrap());
}

#[wasm_bindgen]
pub fn ui_encoder_left() {
    INPUT_QUEUE.with(|q| {
        q.borrow_mut().push(UIInputEvent::EncoderTurn(-1));
    });
}

#[wasm_bindgen]
pub fn ui_encoder_right() {
    INPUT_QUEUE.with(|q| {
        q.borrow_mut().push(UIInputEvent::EncoderTurn(1));
    });
}

#[wasm_bindgen]
pub fn ui_encoder_switch(state: bool) {
    INPUT_QUEUE.with(|q| {
        q.borrow_mut().push(UIInputEvent::EncoderSwitch(state));
    });
}

#[wasm_bindgen]
pub fn midi_new_message(message: &JsValue) {
    let MidiMsgWrapper(msg) = message.into_serde().unwrap();
    MIDI_QUEUE.with(|q| {
        q.borrow_mut().push(msg);
    })
}

// This is like the `main` function, except for JavaScript.
#[wasm_bindgen(start)]
pub async fn main_js() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    let mut program = SequencerProgram::<
        LocalStorageDevice,
        JSTime,
        WebSimulatorDisplay<Rgb565>,
        WebTaskInterface,
    >::new();

    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();

    let output_settings = OutputSettingsBuilder::new()
        .scale(2)
        .pixel_spacing(1)
        .build();

    let display = WebSimulatorDisplay::new(
        (SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32),
        &output_settings,
        document.get_element_by_id("screen").as_ref(),
    );

    let output = BrowserOutput::new();

    program.setup();

    let fs = FileSystem::new(LocalStorageDevice, JSTime).await.unwrap();
    let mut task_manager = TaskManager::new(fs);
    let (mut tm_to_pgm_tx, tm_to_pgm_rx) = mpsc::channel(128);
    let (pgm_to_tm_tx, mut pgm_to_tm_rx) = mpsc::channel(128);

    spawn_local(async move {
        info("Running task manager...");
        task_manager
            .run_tasks(&mut pgm_to_tm_rx, &mut tm_to_pgm_tx)
            .await;
    });

    loop_func(program, output, window, display, tm_to_pgm_rx, pgm_to_tm_tx);

    Ok(())
}
