use serde::{Serialize, Deserialize};
use ufmt::{uDisplay, uWrite, Formatter, uwrite, derive::uDebug};

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

// valid range is the same as MIDI: C-1 - G9
#[derive(uDebug, Debug, PartialEq, Eq, Clone, Copy, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NotePair(pub Note, pub i8);

impl uDisplay for NotePair {
    fn fmt<W>(&self, fmt: &mut Formatter<'_, W>) -> Result<(), W::Error>
    where
        W: uWrite + ?Sized,
    {
        let mut out = heapless::String::<8>::new();
        uwrite!(out, "{}{}", self.0, self.1).unwrap();
        fmt.write_str(&out)
    }
}

impl From<u8> for NotePair {
    fn from(val: u8) -> Self {
        let note = match (val as i8 - 12) % 12 {
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
        NotePair(note, (val as i8 - 12) / 12)
    }
}

pub struct InvalidNotePair;

impl TryFrom<&NotePair> for u8 {
    type Error = InvalidNotePair;
    fn try_from(val: &NotePair) -> Result<u8, InvalidNotePair> {
        let NotePair(n, o) = val;
        if *o < -1 || *o > 9 || (*o == 9 && *n > Note::G) {
            Err(InvalidNotePair)
        } else {
            Ok((*n as u8 & 0x7f) + (o + 1) as u8 * 12)
        }
    }
}