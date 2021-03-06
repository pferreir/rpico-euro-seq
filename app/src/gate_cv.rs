use core::marker::PhantomData;

use embedded_hal::digital::v2::{OutputPin, PinState};
use embedded_hal::spi::MODE_0;
use embedded_time::rate::Extensions;
use mcp49xx::interface::SpiInterface;
use mcp49xx::marker::{DualChannel, Resolution12Bit, Unbuffered};
use mcp49xx::{Channel, Command, Mcp49xx};
use rp2040_hal::gpio::bank0::{Gpio10, Gpio11, Gpio4, Gpio5, Gpio9};
use rp2040_hal::gpio::PushPullOutput;
use rp2040_hal::gpio::{
    pin::{bank0::BankPinId, FunctionSpi},
    Pin, PinId,
};
use rp2040_hal::pac::{RESETS, SPI1};
use rp2040_hal::spi::{Enabled, SpiDevice};
use rp2040_hal::Spi;

use logic::util::{DACVoltage, GateOutput};

pub type GateCVOutWithPins = GateCVOut<SPI1, Gpio10, Gpio11, Gpio9, Gpio4, Gpio5>;

pub struct GateCVOut<
    D: SpiDevice,
    CLK,
    MOSI,
    CS: PinId,
    G0: PinId + BankPinId,
    G1: PinId + BankPinId,
> {
    driver: Mcp49xx<
        SpiInterface<Spi<Enabled, D, 8>, Pin<CS, PushPullOutput>>,
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
        D: SpiDevice,
        CLK: PinId + BankPinId,
        MOSI: PinId + BankPinId,
        CS: PinId + BankPinId,
        G1: PinId + BankPinId,
        G2: PinId + BankPinId,
    > GateCVOut<D, CLK, MOSI, CS, G1, G2>
{
    pub fn new(
        resets: &mut RESETS,
        // DAC
        device: D,
        _clk: Pin<CLK, FunctionSpi>,
        _mosi: Pin<MOSI, FunctionSpi>,
        cs: Pin<CS, PushPullOutput>,
        // gates
        gate1: Pin<G1, PushPullOutput>,
        gate2: Pin<G2, PushPullOutput>,
    ) -> Self {
        let spi1 = Spi::new(device).init(resets, 125_000_000u32.Hz(), 1_000_000u32.Hz(), &MODE_0);

        Self {
            driver: Mcp49xx::new_mcp4822(spi1, cs),
            _clk: PhantomData,
            _mosi: PhantomData,
            _cs: PhantomData,
            gate0: gate1,
            gate1: gate2,
        }
    }
}

impl<
        't,
        D: SpiDevice,
        CLK: PinId + BankPinId,
        MOSI: PinId + BankPinId,
        CS: PinId + BankPinId,
        G1: PinId + BankPinId,
        G2: PinId + BankPinId,
    > GateOutput<'t, DACVoltage> for GateCVOut<D, CLK, MOSI, CS, G1, G2>
{
    fn set_ch0(&mut self, val: DACVoltage) {
        let cmd = Command::default();
        let cmd = cmd.channel(Channel::Ch0).double_gain().value(val.into());
        self.driver.send(cmd).unwrap();
    }

    fn set_ch1(&mut self, val: DACVoltage) {
        let cmd = Command::default();

        let cmd = cmd.channel(Channel::Ch1).double_gain().value(val.into());
        self.driver.send(cmd).unwrap();
    }

    fn set_gate0(&mut self, val: bool) {
        self.gate0
            .set_state(if val { PinState::High } else { PinState::Low })
            .unwrap();
    }

    fn set_gate1(&mut self, val: bool) {
        self.gate1
            .set_state(if val { PinState::High } else { PinState::Low })
            .unwrap();
    }
}
