#![no_std]
#![feature(let_chains)]

pub mod programs;
pub mod ui;
pub mod util;
pub mod screen;

pub mod log {
    extern "Rust" {
        fn _log(text: *const str);
    }

    pub fn info(text: &str) {
        unsafe { _log(text as *const str) };
    }
}
