use core::marker::PhantomData;

use embedded_hal::spi::MODE_0;
use embedded_time::rate::Extensions;
use mcp49xx::interface::SpiInterface;
use mcp49xx::marker::{DualChannel, Resolution12Bit, Unbuffered};
use mcp49xx::{Mcp49xx, Channel, Command};
use rp2040_hal::gpio::{
    pin::{bank0::BankPinId, FunctionSpi},
    Output, Pin, PinId, PushPull,
};
use rp2040_hal::pac::RESETS;
use rp2040_hal::spi::{Enabled, SpiDevice};
use rp2040_hal::Spi;
pub struct Dac<D: SpiDevice, CLK, MOSI, CS: PinId> {
    driver: Mcp49xx<SpiInterface<Spi<Enabled, D, 8>, Pin<CS, Output<PushPull>>>, Resolution12Bit, DualChannel, Unbuffered>,
    _clk: PhantomData<CLK>,
    _mosi: PhantomData<MOSI>,
    _cs: PhantomData<CS>,
}

impl<D: SpiDevice, CLK: PinId + BankPinId, MOSI: PinId + BankPinId, CS: PinId + BankPinId>
    Dac<D, CLK, MOSI, CS>
{
    pub fn new(
        resets: &mut RESETS,
        device: D,
        clk: Pin<CLK, FunctionSpi>,
        mosi: Pin<MOSI, FunctionSpi>,
        cs: Pin<CS, Output<PushPull>>,
    ) -> Self {
        let spi1 = Spi::new(device).init(
            resets,
            125_000_000u32.Hz(),
            1_000_000u32.Hz(),
            &MODE_0,
        );

        Self {
            driver: Mcp49xx::new_mcp4822(spi1, cs),
            _clk: PhantomData,
            _mosi: PhantomData,
            _cs: PhantomData,
        }
    }

    pub fn init(&mut self) {
        let cmd = Command::default();
        let cmd = cmd.double_gain();
        self.driver.send(cmd).unwrap();
    }

    pub fn set_ch0(&mut self, val: u16) {
        let cmd = Command::default();
        let cmd = cmd.channel(Channel::Ch0).value(val);
        self.driver.send(cmd).unwrap();
    }

    pub fn set_ch1(&mut self, val: u16) {
        let cmd = Command::default();

        let cmd = cmd.channel(Channel::Ch1).value(val);
        self.driver.send(cmd).unwrap();
    }
}
