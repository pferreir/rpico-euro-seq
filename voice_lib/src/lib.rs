#![no_std]

use heapless::String;
use ufmt::{derive::uDebug, uDisplay, uWrite, uwrite, Formatter};

#[derive(Copy, Clone, Debug, uDebug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Note {
    C = 0,
    Db = 1,
    D = 2,
    Eb = 3,
    E = 4,
    F = 5,
    Gb = 6,
    G = 7,
    Ab = 8,
    A = 9,
    Bb = 10,
    B = 11,
}

#[derive(Copy, Clone, Debug, uDebug)]
#[repr(u8)]
pub enum NoteFlag {
    None = 0,
    Note = 1,
    Legato = 2
}

impl Into<NoteFlag> for u8 {
    fn into(self) -> NoteFlag {
        match self {
            0 => NoteFlag::None,
            1 => NoteFlag::Note,
            2 => NoteFlag::Legato,
            _ => unreachable!()
        }
    }
}



impl uDisplay for Note {
    fn fmt<W>(&self, fmt: &mut Formatter<'_, W>) -> Result<(), W::Error>
    where
        W: uWrite + ?Sized,
    {
        fmt.write_str(match self {
            Note::C => "C",
            Note::Db => "Db",
            Note::D => "D",
            Note::Eb => "Eb",
            Note::E => "E",
            Note::F => "F",
            Note::Gb => "Gb",
            Note::G => "G",
            Note::Ab => "Ab",
            Note::A => "A",
            Note::Bb => "Bb",
            Note::B => "B",
        })
    }
}

impl Note {
    pub fn is_black_key(&self) -> bool {
        match self {
            Note::Db | Note::Eb | Note::Gb | Note::Ab | Note::Bb => true,
            _ => false,
        }
    }
}

#[derive(uDebug, Debug, PartialEq, Eq, Clone, Copy, PartialOrd, Ord)]
pub struct NotePair(pub Note, pub i8);

impl uDisplay for NotePair {
    fn fmt<W>(&self, fmt: &mut Formatter<'_, W>) -> Result<(), W::Error>
    where
        W: uWrite + ?Sized,
    {
        let mut out = String::<8>::new();
        uwrite!(out, "{}{}", self.0, self.1).unwrap();
        fmt.write_str(&out)
    }
}

pub fn midi_to_note(midi: i8) -> NotePair {
    let note = match (midi - 12) % 12 {
        0 => Note::C,
        1 => Note::Db,
        2 => Note::D,
        3 => Note::Eb,
        4 => Note::E,
        5 => Note::F,
        6 => Note::Gb,
        7 => Note::G,
        8 => Note::Ab,
        9 => Note::A,
        10 => Note::Bb,
        11 => Note::B,
        _ => unreachable!(),
    };
    NotePair(note, (midi as i8 - 12) / 12)
}

impl Into<NotePair> for i8 {
    fn into(self) -> NotePair {
        midi_to_note(self)
    }
}

impl Into<i8> for &NotePair {
    fn into(self) -> i8 {
        let NotePair(n, o) = self;
        (*n as u8 & 0x7f) as i8 + (o + 1) * 12
    }
}

#[derive(Debug)]
pub struct VoiceTrack<const N: usize, const M: usize> {
    notes: [i8; N],
    flags: [u8; M]
}

const fn create_array<const N: usize>() -> [i8; N] {
    let mut array = [0; N];
    array[0] = 72;
    array[1] = 73;
    array[2] = 71;
    array[3] = 75;
    array[4] = 76;
    array[5] = 78;
    array
}

impl<const N: usize, const M: usize> VoiceTrack<N, M> {
    pub fn new() -> Self {
        if N != (M * 4) {
            panic!("N should be equal to 4 times M");
        }
        Self {
            notes: create_array(),
            flags: [0u8; M]
        }
    }

    pub fn set_note<F: Fn(Option<(NotePair, NoteFlag)>) -> (NotePair, NoteFlag)>(&mut self, beat: usize, f: F) {
        let (note, flag) = f(self.get_note(beat));
        self.notes[beat] = (&note).into();
        let idx = beat / 4;
        let sub_idx = beat % 4;
        let bit_mask = 0xc0 >> (sub_idx * 2);
        self.flags[idx] = (self.flags[idx] & !bit_mask) | ((flag as u8) << (7 - sub_idx * 2));
    }

    pub fn get_note(&self, t: usize) -> Option<(NotePair, NoteFlag)> {
        let idx = t / 4;
        let sub_idx = t % 4;
        let flags = (self.flags[idx] & (0xc0 >> (sub_idx * 2))) >> (7 - sub_idx * 2);
        match flags.into() {
            NoteFlag::None => None,
            _ => Some((self.notes[t].into(), flags.into()))
        }

    }

    pub fn since<'t>(&'t self, t: usize, num: usize) -> impl Iterator<Item = (usize, NotePair, NoteFlag)> + 't {
        self.notes[t..(t + num)]
            .iter()
            .enumerate()
            .map(move |(n, e)| {
                let idx = (t + n) / 4;
                let sub_idx = (t + n) % 4;
                let flags = (self.flags[idx] & (0xc0 >> (sub_idx * 2))) >> (7 - sub_idx * 2);
                (t + n, (*e).into(), flags.into())
            })
    }
}

pub enum NoteState {
    On(NotePair),
    Off,
    Legato(NotePair)
}

impl From<&NoteState> for (Option<NotePair>, NoteFlag) {
    fn from(state: &NoteState) -> Self {
        match state {
            NoteState::On(n) => {
                (Some(*n), NoteFlag::Note)
            },
            NoteState::Off => {
                (None, NoteFlag::None)
            },
            NoteState::Legato(n) => {
                (Some(*n), NoteFlag::Legato)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use heapless::String;

    use super::{Note, NotePair, VoiceTrack, VoiceState};
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
        let mut h = VoiceTrack::<10>::new();
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
