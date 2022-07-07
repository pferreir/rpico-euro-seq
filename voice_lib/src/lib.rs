#![no_std]

extern crate alloc;

use core::{fmt, f32::consts::E};
use alloc::vec::Vec;
use heapless::String;
use serde::{Deserialize, Serialize, de::{Visitor, SeqAccess}};
use ufmt::{derive::uDebug, uDisplay, uWrite, uwrite, Formatter};

#[derive(Copy, Clone, Debug, uDebug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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

#[derive(Copy, Clone, Debug, uDebug, PartialEq)]
#[repr(u8)]
pub enum NoteFlag {
    None = 0,
    Note = 1,
    Legato = 2,
}

impl Into<NoteFlag> for u8 {
    fn into(self) -> NoteFlag {
        match self {
            0 => NoteFlag::None,
            1 => NoteFlag::Note,
            2 => NoteFlag::Legato,
            _ => unreachable!(),
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

#[derive(uDebug, Debug, PartialEq, Eq, Clone, Copy, PartialOrd, Ord, Serialize, Deserialize)]
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

pub fn midi_to_note(midi: u8) -> NotePair {
    let note = match (midi as i8 - 12) % 12 {
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

impl Into<NotePair> for u8 {
    fn into(self) -> NotePair {
        midi_to_note(self)
    }
}

impl Into<u8> for &NotePair {
    fn into(self) -> u8 {
        let NotePair(n, o) = self;
        (*n as u8 & 0x7f) + (o + 1) as u8 * 12
    }
}

#[derive(Debug)]
pub struct VoiceTrack {
    notes: Vec<u8>,
    flags: Vec<u8>,
}

fn create_array(len: usize) -> Vec<u8> {
    let mut array = Vec::from_iter(core::iter::repeat(0).take(len));
    array[0] = 72;
    array[1] = 73;
    array[2] = 71;
    array[3] = 75;
    array[4] = 76;
    array[5] = 78;
    array
}

impl VoiceTrack {
    pub fn new(size: usize) -> Self {
        let notes = create_array(size);
        Self {
            notes,
            flags: Vec::from_iter(core::iter::repeat(0x11).take(size / 4)),
        }
    }

    pub fn resize(&mut self, new_size: usize) {
        let delta = new_size - self.len();
        for _ in 0..delta {
            self.notes.push(0);
        }

        for _ in 0..(delta / 4) {
            self.flags.push(0);
        }
    }

    pub fn len(&self) -> usize {
        self.notes.len()
    }

    pub fn set_note(
        &mut self,
        beat: usize,
        (note, flag): (Option<NotePair>, NoteFlag),
    ) {
        self.notes[beat] = (&note.unwrap_or(NotePair(Note::C, -127))).into();
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
            _ => Some((self.notes[t].into(), flags.into())),
        }
    }

    pub fn since<'t>(
        &'t self,
        t: usize,
        num: usize,
    ) -> impl Iterator<Item = (usize, Option<NotePair>, NoteFlag)> + 't {
        self.notes
            .iter()
            .skip(t)
            .take(num)
            .enumerate()
            .map(move |(n, e)| {
                let idx = (t + n) / 4;
                let sub_idx = (t + n) % 4;
                let flags = (self.flags[idx] & (0xc0 >> (sub_idx * 2))) >> (7 - sub_idx * 2);
                let flags: NoteFlag = flags.into();
                (
                    t + n,
                    if flags == NoteFlag::None {
                        None
                    } else {
                        Some((*e).into())
                    },
                    flags.into(),
                )
            })
    }
}

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

impl Serialize for VoiceTrack {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_seq(
            self.since(0, self.len() - 1)
                .map(|(_, np, nf)| -> NoteState { (np, nf).into() }),
        )
    }
}

struct VoiceTrackVisitor;

impl<'de> Visitor<'de> for VoiceTrackVisitor {
    type Value = VoiceTrack;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a sequence of note state values")
    }

    fn visit_seq<V>(self, mut seq: V) -> Result<Self::Value, V::Error>
    where
        V: SeqAccess<'de>,
    {
        let mut size = 16;
        let mut vt = VoiceTrack::new(size);
        let mut n = 0;
        while let Some(e) = seq.next_element::<NoteState>()? {
            vt.set_note(n, e.into());
            n += 1;

            if n > size {
                vt.resize(size * 2);
            }
        }
        Ok(vt)
    }
}

impl<'de> Deserialize<'de> for VoiceTrack {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(VoiceTrackVisitor)
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
