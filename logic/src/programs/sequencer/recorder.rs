use core::marker::PhantomData;
use heapless::{String, Vec};
use ufmt::uwrite;
use voice_lib::{NoteFlag, NotePair, VoiceTrack};

use crate::{log, util::DiscreetUnwrap, stdlib::{StdlibError, TaskType}};

use super::data::SequenceFile;


const NUM_VOICES: usize = 2;
const DEFAULT_SIZE: usize = 16;

pub(crate) struct MonoRecorderBox<'t> {
    file: SequenceFile,
    pub voice_state: VoiceTrack,
    current_note: Vec<NotePair, NUM_VOICES>,
    keys_changed: bool,
    _t: &'t PhantomData<()>,
}

impl<'t> MonoRecorderBox<'t> {
    pub(crate) fn new() -> Self {
        Self {
            file: SequenceFile::new("default"),
            voice_state: VoiceTrack::new(DEFAULT_SIZE),
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
        self.voice_state.set_note(beat, (Some(n), NoteFlag::Note)).duwrp();
        self.keys_changed = true;
        let mut text = String::<32>::new();
        uwrite!(text, "KEY PRESS {}: {:?}", beat, n).unwrap();
        log::debug(&text);
    }

    pub(crate) fn key_released(&mut self, _beat: usize, n: NotePair) {
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
            self.voice_state.set_note(beat, (Some(*n), NoteFlag::Legato)).duwrp();
        }

        // initialize already next note if there is at least a pressed one
        if let Some(n) = self.current_note.last() {
            self.voice_state
                .set_note(beat + 1, (Some(*n), NoteFlag::Legato)).duwrp();
        }
        self.keys_changed = false;
    }

    pub(crate) fn iter_notes_since(
        &'t self,
        t: usize,
        num: usize,
    ) -> impl Iterator<Item = (usize, Option<(Option<NotePair>, NoteFlag)>)> + 't {
        self.voice_state.since(t, num)
    }

    pub(crate) fn set_file_name(&mut self, file_name: &String<12>) {
        self.file.set_name(file_name);
    }

    pub(crate) fn save_file(&mut self) -> Result<TaskType, StdlibError> {
        Ok(self.file.save()?)
    }
}
