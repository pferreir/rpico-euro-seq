#[macro_use]
extern crate lazy_static;

use core::str;
use std::{
    cell::RefCell,
    rc::Rc,
    borrow::Borrow,
};

use embedded_graphics_web_simulator::{
    display::WebSimulatorDisplay, output_settings::OutputSettingsBuilder,
};
use midi_types::{MidiMessage};
use wasm_bindgen::{prelude::*, JsCast};
use web_sys::{Window};
use web_sys::console;
use serde::{Serialize, Deserialize};

use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb565, prelude::*};
use logic::{
    programs::{SequencerProgram, Program},
    screen::{SCREEN_HEIGHT, SCREEN_WIDTH},
    ui::UIInputEvent,
};


#[inline(never)]
#[no_mangle]
unsafe fn _log(text: *const str)  {
    console::info_1(&text.as_ref().unwrap().into());
}
struct MidiMsgWrapper(MidiMessage);

impl<'t> Deserialize<'t> for MidiMsgWrapper {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'t> {
        let (on, c, k, v) = <(bool, u8, u8, u8)>::deserialize(deserializer).unwrap();
        Ok(MidiMsgWrapper(match on {
            true => {
                MidiMessage::NoteOn(c.into(), k.into(), v.into())
            },
            false => {
                MidiMessage::NoteOff(c.into(), k.into(), v.into())
            }
        }))
    }
}

impl Serialize for MidiMsgWrapper {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
        let msg = self.0.clone();
        match msg {
            MidiMessage::NoteOn(c, n, v) => {
                let t: (bool, u8, u8, u8) = (true, c.into(), n.into(), v.into());
                t.serialize(serializer)
            },
            MidiMessage::NoteOff(c, n, v) => {
                let t: (bool, u8, u8, u8) = (false, c.into(), n.into(), v.into());
                t.serialize(serializer)
            },
            _ => {
                unimplemented!()
            }
        }
    }
}

#[global]

// When the `wee_alloc` feature is enabled, this uses `wee_alloc` as the global
// allocator.
//
// If you don't want to use `wee_alloc`, you can safely delete this.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

thread_local! {
    static INPUT_QUEUE: RefCell<Vec<UIInputEvent>> = RefCell::new(Vec::new());
    static MIDI_QUEUE: RefCell<Vec<MidiMessage>> = RefCell::new(Vec::new());
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
    // This provides better error messages in debug mode.
    // It's disabled in release mode so it doesn't bloat up the file size.
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

    loop_func(program, window, display);

    Ok(())
}
