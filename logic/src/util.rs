use core::fmt;
use core::fmt::Debug;
use core::str;
use embedded_midi::Note as MidiNote;
use heapless::spsc::Queue;
use voice_lib::{NotePair, InvalidNotePair};

const MIDI_NOTE_0V: u16 = 36;

pub struct QueuePoppingIter<'t, T, const N: usize> {
    wrapped: &'t mut Queue<T, N>}

impl<'t, T, const N: usize> QueuePoppingIter<'t, T, N> {
    pub fn new(wrapped: &'t mut Queue<T, N>) -> Self {
        Self {
            wrapped
        }
    }
}

impl<'t, T, const N: usize> Iterator for QueuePoppingIter<'t, T, N> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.wrapped.dequeue()
    }

}

pub fn midi_note_to_lib(n: MidiNote) -> NotePair {
    let note: u8 =  n.into();
    note.into()
}

pub struct DACVoltage(u16);

impl From<DACVoltage> for u16 {
    fn from(v: DACVoltage) -> Self {
        v.0
    }
}

impl TryFrom<&NotePair> for DACVoltage {
    type Error = InvalidNotePair;

    fn try_from(value: &NotePair) -> Result<Self, Self::Error> {
        let semitones: u8 = value.try_into()?;
        Ok(DACVoltage((1000 * ((semitones.max(0) as u16).saturating_sub(MIDI_NOTE_0V)) / 12) & 0xfff))
    }
}
pub trait GateOutput<'t, T: TryFrom<&'t NotePair>> {
    fn set_ch0(&mut self, val: T);
    fn set_ch1(&mut self, val: T);
    fn set_gate0(&mut self, val: bool);
    fn set_gate1(&mut self, val: bool);
}


// https://stackoverflow.com/a/64726826
pub struct ByteMutWriter<'a> {
    buf: &'a mut [u8],
    cursor: usize,
}


impl<'a> ByteMutWriter<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        ByteMutWriter { buf, cursor: 0 }
    }

    pub fn as_str(&self) -> &str {
        str::from_utf8(&self.buf[0..self.cursor]).unwrap()
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    pub fn clear(&mut self) {
        self.cursor = 0;
    }

    pub fn len(&self) -> usize {
        self.cursor
    }

    pub fn empty(&self) -> bool {
        self.cursor == 0
    }

    pub fn full(&self) -> bool {
        self.capacity() == self.cursor
    }
}


impl fmt::Write for ByteMutWriter<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let cap = self.capacity();
        for (i, &b) in self.buf[self.cursor..cap]
            .iter_mut()
            .zip(s.as_bytes().iter())
        {
            *i = b;
        }
        self.cursor = usize::min(cap, self.cursor + s.as_bytes().len());
        Ok(())
    }
}

pub(crate) trait DiscreetUnwrap<T, E> {
    fn duwrp(self) -> T;
}

impl<T, E> DiscreetUnwrap<T, E> for Result<T, E> {
    fn duwrp(self) -> T  {
        match self {
            Ok(r) => r,
            Err(_) => {
                panic!("duwrp() failed.")
            },
        }
    }
}
