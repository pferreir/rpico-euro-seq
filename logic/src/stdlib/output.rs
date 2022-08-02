use core::{
    fmt::Debug,
    ops::{Index, IndexMut},
    pin::Pin,
};

use voice_lib::NotePair;

pub enum GateChannelId {
    Gate0,
    Gate1,
}

pub enum CVChannelId {
    CV0,
    CV1,
}

pub trait Output<T: for<'t> TryFrom<&'t NotePair, Error = E>, E> {
    fn set_gate(&mut self, id: GateChannelId, value: bool);
    fn set_cv(&mut self, id: CVChannelId, value: T);
}

pub trait Channel<T> {
    fn set(&mut self, val: T);
}

pub trait GateChannel: Channel<bool> {}

pub trait CVChannel<T>: Channel<T> {
    type Error: Debug;

    fn set_from_note(&mut self, val: &NotePair) -> Result<(), Self::Error>;
}
