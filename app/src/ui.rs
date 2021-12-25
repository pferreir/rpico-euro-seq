use ufmt::derive::uDebug;

#[derive(uDebug, Debug)]
pub enum UIInputEvent {
    EncoderTurn(i8),
    EncoderSwitch(bool),
    Switch1(bool),
    Switch2(bool)
}
