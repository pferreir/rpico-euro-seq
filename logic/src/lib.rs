#![no_std]

#![feature(let_chains)]
#![feature(generic_associated_types)]
#![feature(type_alias_impl_trait)]
#![feature(associated_type_defaults)]
#![feature(associated_type_bounds)]

extern crate alloc;

pub mod stdlib;

pub mod programs;
pub mod ui;
pub mod util;
pub mod screen;

pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

pub mod log {
    use super::LogLevel;

    extern "Rust" {
        fn _log(text: *const str, level: LogLevel);
    }

    pub fn info(text: &str) {
        unsafe { _log(text as *const str, LogLevel::Info) };
    }

    pub fn debug(text: &str) {
        unsafe { _log(text as *const str, LogLevel::Debug) };
    }

    pub fn warning(text: &str) {
        unsafe { _log(text as *const str, LogLevel::Warning) };
    }

    pub fn error(text: &str) {
        unsafe { _log(text as *const str, LogLevel::Error) };
    }
}
