#![feature(generic_associated_types)]
#![feature(type_alias_impl_trait)]

use core::str;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::{borrow::Borrow, cell::RefCell, rc::Rc};

use embedded_graphics_web_simulator::{
    display::WebSimulatorDisplay, output_settings::OutputSettingsBuilder,
};
use embedded_sdmmc::TimeSource;
use embedded_sdmmc::{Block, BlockCount, BlockDevice, BlockIdx, Controller, Timestamp};
use js_sys::{Date, Uint8Array};
use logic::stdlib::{FileSystem, TaskManager};
use midi_types::MidiMessage;
use serde::{Deserialize, Serialize};
use ufmt::{uDebug, uWrite};
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::{future_to_promise, spawn_local};
use web_sys::OscillatorNode;
use web_sys::Window;
use web_sys::{console, RequestMode};
use web_sys::{AudioContext, Request};
use web_sys::{GainNode, OscillatorType, RequestInit};

use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb565, prelude::*};
use logic::util::GateOutput;
use logic::{log, LogLevel};
use logic::{
    programs::{Program, SequencerProgram},
    screen::{SCREEN_HEIGHT, SCREEN_WIDTH},
    ui::UIInputEvent,
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

struct BrowserOutput {
    osc0: OscillatorNode,
    vol0: GainNode,
    state0: bool,
}

struct Frequency(f32);

impl TryFrom<&NotePair> for Frequency {
    type Error = InvalidNotePair;

    fn try_from(np: &NotePair) -> Result<Self, Self::Error> {
        let n: u8 = np.try_into()?;
        Ok(Self(440.0 * 2f32.powf((n as f32 - 69.0) / 12.0)))
    }
}

impl<'t> GateOutput<'t, Frequency> for BrowserOutput {
    fn set_ch0(&mut self, val: Frequency) {
        self.osc0.frequency().set_value(val.0);
    }

    fn set_ch1(&mut self, val: Frequency) {
        todo!()
    }

    fn set_gate0(&mut self, val: bool) {
        if val {
            self.vol0.gain().set_value(1.0);
        } else {
            self.vol0.gain().set_value(0.0);
        }
        self.state0 = val;
    }

    fn set_gate1(&mut self, val: bool) {
        todo!()
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
            osc0,
            vol0,
            state0: false,
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

    fn read<'a>(
        &'a mut self,
        blocks: &'a mut [Block],
        BlockIdx(start_block_idx): BlockIdx,
        reason: &str,
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

    fn num_blocks(&self) -> Result<BlockCount, Self::Error> {
        Ok(BlockCount(LOCAL_STORAGE_SIZE as u32 / 512))
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
    P: Program<'t, LocalStorageDevice, WebSimulatorDisplay<Rgb565>, JSTime> + 'static,
>(
    program: P,
    fs: FileSystem<LocalStorageDevice, JSTime>,
    mut output: BrowserOutput,
    window: Window,
    mut display: WebSimulatorDisplay<Rgb565>,
) {
    let task_manager = TaskManager::new(fs);
    let f = Rc::new(RefCell::new(None));
    let g = f.clone();

    let program = Arc::new(Mutex::new(program));
    let task_manager = Arc::new(Mutex::new(task_manager));

    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        // if do_break {

        //     // Drop our handle to this closure so that it will get cleaned
        //     // up once we return.
        //     let _ = f.borrow_mut().take();
        //     return;
        // }
        let now = window
            .performance()
            .expect("should have a Performance")
            .now();

        {
            let mut mg = program.lock().unwrap();
            INPUT_QUEUE.with(|vec| {
                for msg in vec.borrow().iter() {
                    mg.process_ui_input(msg).unwrap();
                }
                vec.borrow_mut().clear();
            });

            MIDI_QUEUE.with(|vec| {
                for msg in vec.borrow().iter() {
                    mg.process_midi(msg);
                }
                vec.borrow_mut().clear();
            });

            let tm = task_manager.clone();
            let tmg = tm.lock();
            mg.run(now.floor() as u32, tmg.unwrap());
        }

        let pgm = program.clone();
        let tkm = task_manager.clone();
        spawn_local(async move {
            let mut mg_pgm = pgm.lock().unwrap();
            let mut mg_tkm = tkm.lock().unwrap();
            mg_tkm.run_tasks(&mut mg_pgm).await;
        });

        {
            let mut mg = program.lock().unwrap();
            display.clear(Rgb565::BLACK).unwrap();
            mg.render_screen(&mut display);
            display.flush().expect("could not flush buffer");
            mg.update_output(&mut output);
        }

        let b: &Rc<RefCell<Option<Closure<dyn FnMut()>>>> = f.borrow();
        // Schedule ourself for another requestAnimationFrame callback.
        request_animation_frame(b.as_ref().borrow().as_ref().unwrap());
    }) as Box<dyn FnMut()>));

    let b: &Rc<RefCell<Option<Closure<dyn FnMut()>>>> = g.borrow();
    request_animation_frame(b.as_ref().borrow().as_ref().unwrap());
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

    let mut program =
        SequencerProgram::<LocalStorageDevice, JSTime, WebSimulatorDisplay<Rgb565>>::new();

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

    loop_func(program, fs, output, window, display);

    Ok(())
}
