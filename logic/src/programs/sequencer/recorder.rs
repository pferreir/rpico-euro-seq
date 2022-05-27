use core::marker::PhantomData;
use heapless::{spsc::Queue, String, Vec};
use ufmt::uwrite;
use voice_lib::{NoteFlag, NotePair, VoiceTrack};

use crate::log;


const NUM_VOICES: usize = 2;
const HISTORY_SIZE: usize = 1024;
const HISTORY_SIZE_DIV_4: usize = 256;


pub(crate) struct MonoRecorderBox<'t> {
    voice_state: VoiceTrack<HISTORY_SIZE, HISTORY_SIZE_DIV_4>,
    current_note: Vec<NotePair, NUM_VOICES>,
    keys_changed: bool,
    _t: &'t PhantomData<()>,
}

impl<'t> MonoRecorderBox<'t> {
    pub(crate) fn new() -> Self {
        Self {
            voice_state: VoiceTrack::new(),
            current_note: Vec::new(),
            keys_changed: false,
            _t: &PhantomData,
        }
    }

    pub(crate) fn last_note(&self) -> Option<&NotePair> {
        self.current_note.last()
    }

    pub(crate) fn key_pressed(&mut self, beat: usize, n: NotePair) {
        self.current_note.push(n).unwrap();
        self.voice_state.set_note(beat, |_| (n, NoteFlag::Note));
        self.keys_changed = true;
        let mut text = String::<32>::new();
        uwrite!(text, "KEY PRESS {}: {:?}", beat, n).unwrap();
        log::debug(&text);
    }

    pub(crate) fn key_released(&mut self, beat: usize, n: NotePair) {
        self.current_note = self
            .current_note
            .iter()
            .filter(|e| *e != &n)
            .cloned()
            .collect();
        self.keys_changed = true;
    }

    pub(crate) fn beat(&mut self, beat: usize) {
        if !self.keys_changed && let Some(n) = self.current_note.last() {
            self.voice_state.set_note(beat, |_| (*n, NoteFlag::Legato));
        }

        // initialize already next note if there is at least a pressed one
        if let Some(n) = self.current_note.last() {
            self.voice_state
                .set_note(beat + 1, |_| (*n, NoteFlag::Legato));
        }
        self.keys_changed = false;
    }

    pub(crate) fn iter_notes_since(
        &'t self,
        t: usize,
        num: usize,
    ) -> impl Iterator<Item = (usize, NotePair, NoteFlag)> + 't {
        self.voice_state.since(t, num)
    }
}
