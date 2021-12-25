use core::marker::PhantomData;

use embedded_midi::Note;
use heapless::spsc::Queue;
use voice_lib::NotePair;

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

pub fn midi_note_to_lib(n: Note) -> NotePair {
    let note: u8 =  n.into();
    (note as i8).into()
}
