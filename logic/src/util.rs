use core::{fmt, ops::IndexMut};
use core::str;
use embedded_midi::Note as MidiNote;
use heapless::spsc::Queue;
use voice_lib::{NotePair, InvalidNotePair};


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
