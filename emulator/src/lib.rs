use core::str;
use std::{borrow::Borrow, cell::RefCell, rc::Rc};

use embedded_graphics_web_simulator::{
    display::WebSimulatorDisplay, output_settings::OutputSettingsBuilder,
};
use logic::log;
use midi_types::MidiMessage;
use serde::{Deserialize, Serialize};
use wasm_bindgen::{prelude::*, JsCast};
use web_sys::AudioContext;
use web_sys::OscillatorNode;
use web_sys::console;
use web_sys::Window;
use web_sys::{OscillatorType, GainNode};

use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb565, prelude::*};
use logic::LogLevel;
use logic::util::GateOutput;
use logic::{
    programs::{Program, SequencerProgram},
    screen::{SCREEN_HEIGHT, SCREEN_WIDTH},
    ui::UIInputEvent,
};
use voice_lib::NotePair;

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

impl From<&NotePair> for Frequency {
    fn from(np: &NotePair) -> Self {
        let n: u8 = np.into();
        Self(440.0 * 2f32.powf((n as f32 - 69.0) / 12.0))
    }
}

impl<'t> GateOutput<'t, Frequency> for BrowserOutput {
    fn set_ch0(&mut self, val: Frequency) {
        log::info("SET FREQ");
        self.osc0.frequency().set_value(val.0);
    }

    fn set_ch1(&mut self, val: Frequency) {
        todo!()
    }

    fn set_gate0(&mut self, val: bool) {
        log::info("SET GATE");
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
        Self { osc0, vol0, state0: false }
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

fn loop_func<P: Program + 'static>(
    mut program: P,
    mut output: BrowserOutput,
    window: Window,
    mut display: WebSimulatorDisplay<Rgb565>,
) {
    let f = Rc::new(RefCell::new(None));
    let g = f.clone();

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

        INPUT_QUEUE.with(|vec| {
            for msg in vec.borrow().iter() {
                program.process_ui_input(msg);
            }
            vec.borrow_mut().clear();
        });

        MIDI_QUEUE.with(|vec| {
            for msg in vec.borrow().iter() {
                program.process_midi(msg);
            }
            vec.borrow_mut().clear();
        });

        program.run(now.floor() as u32);

        display.clear(Rgb565::BLACK).unwrap();
        program.render_screen(&mut display);
        display.flush().expect("could not flush buffer");
        program.update_output(&mut output);

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
pub fn main_js() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    let program = SequencerProgram::new();

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

    loop_func(program, output, window, display);

    Ok(())
}
