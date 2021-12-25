#![no_std]

use core::mem;
use core::ptr;
use core::{marker::PhantomData, ops::Index};

use heapless::{spsc::Queue, String, Vec};
use ufmt::{derive::uDebug, uDisplay, uWrite, uwrite, Formatter};

macro_rules! make_array {
    ($n:expr, $constructor:expr) => {{
        unsafe {
            let mut items: [_; $n] = mem::uninitialized();
            for (i, place) in items.iter_mut().enumerate() {
                ptr::write(place, $constructor(i));
            }
            items
        }
    }};
}

#[derive(Copy, Clone, Debug, uDebug, PartialEq)]
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
    A = 9 ,
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

#[derive(uDebug, Debug, PartialEq, Clone, Copy)]
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
    let note = match (midi - 24) % 12 {
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

type VoiceLine = (NotePair, u32, Option<u32>);

#[derive(Debug)]
pub struct VoiceHistory<const N: usize> {
    content: Vec<VoiceLine, N>,
}

impl<const N: usize> VoiceHistory<N> {
    pub fn new() -> Self {
        Self {
            content: Vec::<VoiceLine, N>::new(),
        }
    }

    pub fn start_note(&mut self, n: NotePair, t: u32) {
        self.content.push((n, t, None)).unwrap();
    }

    pub fn end_note(&mut self, t: u32) {
        let last = self.content.pop();
        assert!(last.is_some());
        let (n, start, end) = last.unwrap();
        assert!(end.is_none());
        self.content.push((n, start, Some(t))).unwrap();
    }

    pub fn last(&self) -> Option<&VoiceLine> {
        self.content.last()
    }

    pub fn since(&self, t: u32) -> impl Iterator<Item=&(NotePair, u32, Option<u32>)> {
        self.content.iter().filter(move |entry| {
            entry.1 >= t || entry.2.unwrap_or(0) >= t
        })
    }
}

pub struct VoiceState<'t, const NUM_VOICES: usize, const SIZE_HISTORY: usize> {
    queue: [Option<NotePair>; NUM_VOICES],
    history: [VoiceHistory<SIZE_HISTORY>; NUM_VOICES],
    _t: &'t PhantomData<()>,
}

impl<'t, const NUM_VOICES: usize, const SIZE_HISTORY: usize>
    VoiceState<'t, NUM_VOICES, SIZE_HISTORY>
{
    pub fn new() -> Self {
        Self {
            queue: make_array!(NUM_VOICES, |_| None),
            history: make_array!(NUM_VOICES, |_| VoiceHistory::new()),
            _t: &PhantomData,
        }
    }

    pub fn set(&mut self, n: NotePair, now: u32) {
        if let Some((q, h)) = self
            .queue
            .iter_mut()
            .zip(self.history.iter_mut())
            .find(|(q, _)| q.is_none())
        {
            *q = Some(n);
            h.start_note(n, now);
        } else {
            // find voice which started the earliest
            let (q, h) = self
                .queue
                .iter_mut()
                .zip(self.history.iter_mut())
                .min_by_key(|(_, h)| h.last().unwrap().1)
                .unwrap();
            assert!(q.is_some());
            q.replace(n);

            // replace earliest voice
            h.end_note(now);
            h.start_note(n, now);
        }
    }

    pub fn clear(&mut self, n: NotePair, now: u32) {
        let voice = self
            .queue
            .iter_mut()
            .zip(self.history.iter_mut())
            .find(|(v, _)| {
                if let Some(np) = v {
                    *np == n
                } else {
                    false
                }
            });
        if let Some((q, h)) = voice {
            *q = None;
            h.end_note(now);
        }
        // otherwise, this is a "zombie voice" which will die silently
    }

    pub fn since(&self, t: u32) -> impl Iterator<Item=&(NotePair, u32, Option<u32>)> {
        self.history.iter().flat_map(move |h| h.since(t))
    }

    pub fn iter_voices(&self) -> impl Iterator<Item=&Option<NotePair>> {
        self.queue.iter()
    }
}

impl<'t, const NUM_VOICES: usize, const SIZE_HISTORY: usize> Index<u8>
    for VoiceState<'t, NUM_VOICES, SIZE_HISTORY>
where
    Self: 't,
{
    type Output = Option<NotePair>;

    fn index(&self, index: u8) -> &Self::Output {
        &self.queue[index as usize]
    }
}

#[cfg(test)]
mod tests {
    use heapless::String;

    use super::{Note, NotePair, VoiceHistory, VoiceState};
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
        let mut h = VoiceHistory::<10>::new();
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
