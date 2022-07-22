#![no_std]

extern crate alloc;

use serde::{Deserialize, Serialize, de::{Visitor, SeqAccess}};

mod note;
mod track;

pub use note::{Note, NotePair, InvalidNotePair};
pub use track::{NoteFlag, VoiceTrack};


#[derive(Serialize, Deserialize)]
pub enum NoteState {
    On(NotePair),
    Off,
    Legato(NotePair),
}

impl From<NoteState> for (Option<NotePair>, NoteFlag) {
    fn from(state: NoteState) -> Self {
        match state {
            NoteState::On(n) => (Some(n), NoteFlag::Note),
            NoteState::Off => (None, NoteFlag::None),
            NoteState::Legato(n) => (Some(n), NoteFlag::Legato),
        }
    }
}

impl From<(Option<NotePair>, NoteFlag)> for NoteState {
    fn from((np, nf): (Option<NotePair>, NoteFlag)) -> Self {
        match nf {
            NoteFlag::None => NoteState::Off,
            NoteFlag::Note => NoteState::On(np.unwrap()),
            NoteFlag::Legato => NoteState::Legato(np.unwrap()),
        }
    }
}

#[cfg(test)]
mod tests {
    use heapless::String;

    use super::{Note, NotePair, VoiceState, VoiceTrack};
    use ufmt::uwrite;

    #[test]
    fn test_note_midi_conversion() {
        assert!(Into::<NotePair>::into(24) == NotePair(Note::C, 1));
        assert!(Into::<NotePair>::into(50) == NotePair(Note::D, 3));
        assert!(Into::<i8>::into(NotePair(Note::D, 3)) == 50);
    }

    #[test]
    fn test_note_pair_display() {
        let np = NotePair(Note::C, 1);
        let mut out = String::<6>::new();
        uwrite!(out, "{}", np).unwrap();
        assert!(out == "C1");
    }

    #[test]
    fn test_voice_history() {
        let mut h = VoiceTrack::<10>::new(16);
        h.start_note(NotePair(Note::D, 2), 10);

        assert!(h.content.last() == Some(&(NotePair(Note::D, 2), 10, None)));

        h.end_note(20);

        assert!(h.content.last() == Some(&(NotePair(Note::D, 2), 10, Some(20))));

        h.start_note(NotePair(Note::D, 2), 40);

        assert!(h.content.last() == Some(&(NotePair(Note::D, 2), 40, None)));
    }

    #[test]
    fn test_voice_state() {
        let mut s = VoiceState::<2, 10>::new();
        s.set(NotePair(Note::B, 3), 10);
        assert!(s[0] == Some(NotePair(Note::B, 3)));
        assert!(s[1] == None);
        s.clear(NotePair(Note::B, 3), 20);
        assert!(s[0] == None);
        assert!(s[1] == None);
        s.set(NotePair(Note::B, 3), 10);
        s.set(NotePair(Note::C, 4), 12);
        assert!(s[0] == Some(NotePair(Note::B, 3)));
        assert!(s[1] == Some(NotePair(Note::C, 4)));
        assert!(s.history[0].content.last() == Some(&(NotePair(Note::B, 3), 10, None)));
        s.set(NotePair(Note::D, 2), 14);
        assert!(s[0] == Some(NotePair(Note::D, 2)));
    }

    #[test]
    fn test_voice_state_zombie() {
        let mut s = VoiceState::<2, 10>::new();
        s.set(NotePair(Note::D, 2), 10);
        s.set(NotePair(Note::E, 2), 11);
        s.set(NotePair(Note::F, 2), 12);
        s.clear(NotePair(Note::F, 2), 13);
        s.clear(NotePair(Note::E, 2), 14);
        // this shouldn't blow up
        s.clear(NotePair(Note::D, 2), 15);
    }
}
