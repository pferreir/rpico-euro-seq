use embedded_midi::Note;
use heapless::spsc::Queue;
use voice_lib::NotePair;

const MIDI_NOTE_0V: u16 = 36;

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

pub struct DACVoltage(u16);

impl From<DACVoltage> for u16 {
    fn from(v: DACVoltage) -> Self {
        v.0
    }
}

impl From<&NotePair> for DACVoltage {
    fn from(np: &NotePair) -> Self {
        let semitones: i8 = np.into();
        DACVoltage((1000 * ((semitones.max(0) as u16).saturating_sub(MIDI_NOTE_0V)) / 12) & 0xfff)
    }
}
