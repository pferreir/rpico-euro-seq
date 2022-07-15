use alloc::vec::Vec;
use core::fmt;
use serde::{
    de::{Error, SeqAccess, Visitor},
    ser::Error as SerError,
    Deserialize, Serialize,
};
use ufmt::derive::uDebug;

use crate::{InvalidNotePair, Note, NotePair, NoteState};

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

#[derive(Debug)]
pub struct VoiceTrack {
    notes: Vec<u8>,
    flags: Vec<u8>,
}

impl VoiceTrack {
    pub fn new(size: usize) -> Self {
        Self {
            notes: Vec::from_iter(core::iter::repeat(0).take(size)),
            flags: Vec::from_iter(core::iter::repeat(0).take(size / 4)),
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
    ) -> Result<(), InvalidNotePair> {
        self.notes[beat] = (&note.unwrap_or(NotePair(Note::C, -127))).try_into()?;
        let idx = beat / 4;
        let sub_idx = beat % 4;
        let bit_mask = 0xc0 >> (sub_idx * 2);
        self.flags[idx] = (self.flags[idx] & !bit_mask) | ((flag as u8) << (6 - sub_idx * 2));
        Ok(())
    }

    pub fn get_note(&self, t: usize) -> Option<(Option<NotePair>, NoteFlag)> {
        if t >= self.len() {
            None
        } else {
            let idx = t / 4;
            let sub_idx = t % 4;
            let flags = (self.flags[idx] & (0xc0 >> (sub_idx * 2))) >> (6 - sub_idx * 2);
            Some(match flags.into() {
                NoteFlag::None => (None, NoteFlag::None),
                f @ NoteFlag::Legato | f @ NoteFlag::Note => (Some(self.notes[t].into()), f),
            })
        }
        
    }

    pub fn since<'t>(
        &'t self,
        t: usize,
        num: usize,
    ) -> impl Iterator<Item = (usize, Option<(Option<NotePair>, NoteFlag)>)> + 't {
        (t..(t + num)).map(move |n| {
            (n, self.get_note(n))
        })
    }
}

impl Serialize for VoiceTrack {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_seq(
            self.since(0, self.len() - 1)
                .map(|(_, elem)| -> Result<NoteState, S::Error> {
                    let (np, nf) = elem.ok_or(S::Error::custom("Value should not be empty"))?;
                    Ok((np, nf).into())
                }
            ).collect::<Result<Vec<_>, S::Error>>()
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
            vt.set_note(n, e.into())
                .map_err(|_| V::Error::custom("Value is not a valid note"))?;
            n += 1;

            if n > size {
                vt.resize(size * 2);
                size *= 2;
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
