use core::cell::{RefCell, RefMut};
use core::fmt::Debug;
use core::marker::PhantomData;

use critical_section::{Mutex, with};
use embedded_hal::blocking::spi::Write;
use embedded_hal::digital::v2::{OutputPin, PinState};
use logic::stdlib::{CVChannel, CVChannelId, Channel, GateChannel, GateChannelId, Output};
use mcp49xx::interface::SpiInterface;
use mcp49xx::marker::{DualChannel, Resolution12Bit, Unbuffered};
use mcp49xx::{Channel as MCPChannel, Command, Mcp49xx};
use rp2040_hal::gpio::bank0::{Gpio10, Gpio11, Gpio4, Gpio5, Gpio9};
use rp2040_hal::gpio::PushPullOutput;
use rp2040_hal::gpio::{
    pin::{bank0::BankPinId, FunctionSpi},
    Pin, PinId,
};

use voice_lib::{InvalidNotePair, NotePair};

pub type GateCVOutWithPins<SPI> = GateCVOut<SPI, Gpio10, Gpio11, Gpio9, Gpio4, Gpio5>;

const MIDI_NOTE_0V: u16 = 36;

#[derive(Default, Copy, Clone)]
pub struct DACVoltage(u16);

impl From<DACVoltage> for u16 {
    fn from(v: DACVoltage) -> Self {
        v.0
    }
}

impl From<u16> for DACVoltage {
    fn from(v: u16) -> Self {
        Self(v)
    }
}

impl TryFrom<&NotePair> for DACVoltage {
    type Error = InvalidNotePair;

    fn try_from(value: &NotePair) -> Result<Self, Self::Error> {
        let semitones: u8 = value.try_into()?;
        Ok(DACVoltage(
            (1000 * ((semitones.max(0) as u16).saturating_sub(MIDI_NOTE_0V)) / 12) & 0xfff,
        ))
    }
}

#[derive(Default)]
pub struct StoredGateChannel(bool);
impl GateChannel for StoredGateChannel {}

impl Channel<bool> for StoredGateChannel {
    fn set(&mut self, val: bool) {
        self.0 = val;
    }
}

#[derive(Default)]
pub struct StoredCVChannel(DACVoltage);
impl CVChannel<DACVoltage> for StoredCVChannel {
    type Error = InvalidNotePair;

    fn set_from_note(&mut self, val: &NotePair) -> Result<(), Self::Error> {
        self.set(val.try_into()?);
        Ok(())
    }
}

impl Channel<DACVoltage> for StoredCVChannel {
    fn set(&mut self, val: DACVoltage) {
        self.0 = val;
    }
}

pub static OUTPUTS: Mutex<
    RefCell<
        Option<(
            (StoredGateChannel, StoredCVChannel),
            (StoredGateChannel, StoredCVChannel),
        )>,
    >,
> = Mutex::new(RefCell::new(None));

pub struct GateCVOut<
    SPI: Write<u8>,
    CLK,
    MOSI,
    CS: PinId,
    G0: PinId + BankPinId,
    G1: PinId + BankPinId,
> {
    driver: Mcp49xx<
        SpiInterface<SPI, Pin<CS, PushPullOutput>>,
        Resolution12Bit,
        DualChannel,
        Unbuffered,
    >,
    _clk: PhantomData<CLK>,
    _mosi: PhantomData<MOSI>,
    _cs: PhantomData<CS>,
    gate0: Pin<G0, PushPullOutput>,
    gate1: Pin<G1, PushPullOutput>,
}

impl<
        SPI: Write<u8>,
        CLK: PinId + BankPinId,
        MOSI: PinId + BankPinId,
        CS: PinId + BankPinId,
        G1: PinId + BankPinId,
        G2: PinId + BankPinId,
    > GateCVOut<SPI, CLK, MOSI, CS, G1, G2>
where
    SPI::Error: Debug,
{
    pub fn new(
        // DAC
        spi: SPI,
        _clk: Pin<CLK, FunctionSpi>,
        _mosi: Pin<MOSI, FunctionSpi>,
        cs: Pin<CS, PushPullOutput>,
        // gates
        gate1: Pin<G1, PushPullOutput>,
        gate2: Pin<G2, PushPullOutput>,
    ) -> Self {
        Self {
            driver: Mcp49xx::new_mcp4822(spi, cs),
            _clk: PhantomData,
            _mosi: PhantomData,
            _cs: PhantomData,
            gate0: gate1,
            gate1: gate2,
        }
    }

    pub fn update(&mut self) {
        let ((gate0, cv0), (gate1, cv1)) = with(|cs| {
            let v = OUTPUTS.borrow(cs).borrow();
            let out = v.as_ref().unwrap();
            ((out.0 .0 .0, out.0 .1 .0), (out.1 .0 .0, out.1 .1 .0))
        });

        // channel 0
        self.gate0
            .set_state(if gate0 { PinState::High } else { PinState::Low })
            .unwrap();

        let cmd = Command::default();
        let cmd = cmd.channel(MCPChannel::Ch0).double_gain().value(cv0.into());
        self.driver.send(cmd).unwrap();

        // channel 1
        self.gate1
            .set_state(if gate1 { PinState::High } else { PinState::Low })
            .unwrap();

        let cmd = Command::default();
        let cmd = cmd.channel(MCPChannel::Ch1).double_gain().value(cv1.into());
        self.driver.send(cmd).unwrap();
    }
}

pub struct GateCVProxy;

impl GateCVProxy {
    pub fn new() -> Self {
        with(|cs| {
            *OUTPUTS.borrow(cs).borrow_mut() = Some((
                (Default::default(), Default::default()),
                (Default::default(), Default::default()),
            ));
        });
        Self
    }
}

impl<'t> Output<DACVoltage, InvalidNotePair> for GateCVProxy {
    fn set_gate(&mut self, id: GateChannelId, value: bool) {
        with(|cs| {
            let mut val = OUTPUTS.borrow(cs).borrow_mut();
            let v = val.as_mut().unwrap();

            match id {
                GateChannelId::Gate0 => {
                    v.0 .0.set(value);
                }
                GateChannelId::Gate1 => {
                    v.1 .0.set(value);
                }
            }
        });
    }

    fn set_cv(&mut self, id: logic::stdlib::CVChannelId, value: DACVoltage) {
        with(|cs| {
            let mut val = OUTPUTS.borrow(cs).borrow_mut();
            let v = val.as_mut().unwrap();
            match id {
                CVChannelId::CV0 => {
                    v.0 .1.set(value.into());
                }
                CVChannelId::CV1 => {
                    v.1 .1.set(value.into());
                }
            }
        });
    }
}
