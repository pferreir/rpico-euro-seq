use embedded_hal::blocking::delay::DelayMs;
use embedded_hal::blocking::spi::{Write, WriteIter};
use embedded_hal::digital::v2::OutputPin;

#[derive(Debug, Clone, Copy)]
pub enum Instruction {
    NOP = 0x00,
    SWRESET = 0x01,
    RDDID = 0x04,
    RDDST = 0x09,
    SLPIN = 0x10,
    SLPOUT = 0x11,
    PTLON = 0x12,
    NORON = 0x13,
    INVOFF = 0x20,
    INVON = 0x21,
    DISPOFF = 0x28,
    DISPON = 0x29,
    CASET = 0x2A,
    RASET = 0x2B,
    RAMWR = 0x2C,
    RAMRD = 0x2E,
    PTLAR = 0x30,
    COLMOD = 0x3A,
    MADCTL = 0x36,
    FRMCTR1 = 0xB1,
    FRMCTR2 = 0xB2,
    FRMCTR3 = 0xB3,
    INVCTR = 0xB4,
    DISSET5 = 0xB6,
    PWCTR1 = 0xC0,
    PWCTR2 = 0xC1,
    PWCTR3 = 0xC2,
    PWCTR4 = 0xC3,
    PWCTR5 = 0xC4,
    VMCTR1 = 0xC5,
    RDID1 = 0xDA,
    RDID2 = 0xDB,
    RDID3 = 0xDC,
    RDID4 = 0xDD,
    PWCTR6 = 0xFC,
    GMCTRP1 = 0xE0,
    GMCTRN1 = 0xE1,
}

pub struct ST7735<SPI, DC, RST, BLT>
where
    SPI: Write<u8> + WriteIter<u8>,
    DC: OutputPin,
    RST: OutputPin,
    BLT: OutputPin,
{
    spi: SPI,
    // Display interface
    dc: DC,
    // Reset pin.
    rst: Option<RST>,
    // Backlight pin,
    cs: BLT,
    // Visible size (x, y)
    inverted: bool,
    dx: u16,
    dy: u16,
    size_x: u16,
    size_y: u16,
    // Current orientation
    orientation: Orientation,
}

///
/// Display orientation.
///
#[repr(u8)]
#[derive(Copy, Clone)]
pub enum Orientation {
    Portrait = 0b0000_0000,         // no inverting
    Landscape = 0b0110_0000,        // invert column and page/column order
    PortraitSwapped = 0b1100_0000,  // invert page and column order
    LandscapeSwapped = 0b1010_0000, // invert page and page/column order
}

impl Default for Orientation {
    fn default() -> Self {
        Self::Portrait
    }
}

#[derive(Copy, Clone, Debug)]
pub enum BacklightState {
    On,
    Off,
}

#[derive(Debug)]
pub enum Error<PinE> {
    DisplayError,
    Pin(PinE),
}

impl<SPI, DC, RST, BLT, PinE> ST7735<SPI, DC, RST, BLT>
where
    SPI: Write<u8> + WriteIter<u8>,
    DC: OutputPin<Error = PinE>,
    RST: OutputPin<Error = PinE>,
    BLT: OutputPin<Error = PinE>,
{
    pub fn new(spi: SPI, dc: DC, rst: Option<RST>, cs: BLT, size_x: u16, size_y: u16) -> Self {
        Self {
            spi,
            dc,
            rst,
            cs,
            size_x,
            size_y,
            dx: 0,
            dy: 0,
            inverted: false,
            orientation: Orientation::default(),
        }
    }

    pub fn init(&mut self, delay: &mut impl DelayMs<u32>) -> Result<(), Error<PinE>> {
        self.hard_reset(delay)?;
        self.write_command(Instruction::SWRESET, &[])?;
        delay.delay_ms(200);
        self.write_command(Instruction::SLPOUT, &[])?;
        delay.delay_ms(200);
        self.write_command(Instruction::FRMCTR1, &[0x01, 0x2C, 0x2D])?;
        self.write_command(Instruction::FRMCTR2, &[0x01, 0x2C, 0x2D])?;
        self.write_command(Instruction::FRMCTR3, &[0x01, 0x2C, 0x2D, 0x01, 0x2C, 0x2D])?;
        self.write_command(Instruction::INVCTR, &[0x07])?;
        self.write_command(Instruction::PWCTR1, &[0xA2, 0x02, 0x84])?;
        self.write_command(Instruction::PWCTR2, &[0xC5])?;
        self.write_command(Instruction::PWCTR3, &[0x0A, 0x00])?;
        self.write_command(Instruction::PWCTR4, &[0x8A, 0x2A])?;
        self.write_command(Instruction::PWCTR5, &[0x8A, 0xEE])?;
        self.write_command(Instruction::VMCTR1, &[0x0E])?;
        if self.inverted {
            self.write_command(Instruction::INVON, &[])?;
        } else {
            self.write_command(Instruction::INVOFF, &[])?;
        }
        self.write_command(Instruction::MADCTL, &[0x00])?;
        self.write_command(Instruction::COLMOD, &[0x05])?;
        self.write_command(Instruction::DISPON, &[])?;
        delay.delay_ms(200);
        Ok(())
    }

    pub fn hard_reset(&mut self, delay: &mut impl DelayMs<u32>) -> Result<(), Error<PinE>> {
        if let Some(rst) = self.rst.as_mut() {
            rst.set_high().map_err(Error::Pin)?;
            delay.delay_ms(10);
            rst.set_low().map_err(Error::Pin)?;
            delay.delay_ms(10);
            rst.set_high().map_err(Error::Pin)?;
        }

        Ok(())
    }

    pub fn orientation(&self) -> Orientation {
        self.orientation
    }

    pub fn set_orientation(&mut self, orientation: &Orientation) -> Result<(), Error<PinE>> {
        self.orientation = *orientation;
        self.write_command(Instruction::MADCTL, &[*orientation as u8])?;
        Ok(())
    }

    pub fn release(self) -> (DC, Option<RST>, BLT) {
        (self.dc, self.rst, self.cs)
    }

    pub fn write_command(
        &mut self,
        command: Instruction,
        params: &[u8],
    ) -> Result<(), Error<PinE>> {
        self.cs.set_low().map_err(Error::Pin)?;
        self.dc.set_low().map_err(Error::Pin)?;
        self.spi
            .write(&[command as u8])
            .map_err(|_| Error::DisplayError)?;

        if !params.is_empty() {
            self.signal_data()?;
            self.write_data(params)?;
        }

        Ok(())
    }

    pub fn signal_data(&mut self) -> Result<(), Error<PinE>> {
        self.cs.set_low().map_err(Error::Pin)?;
        self.dc.set_high().map_err(Error::Pin)
    }

    pub fn write_data(&mut self, data: &[u8]) -> Result<(), Error<PinE>> {
        self.signal_data()?;
        self.spi
            .write_iter(data.iter().cloned())
            .map_err(|_| Error::DisplayError)
    }

    fn write_word(&mut self, value: u16) -> Result<(), Error<PinE>> {
        self.write_data(&value.to_be_bytes())
    }

    // Sets the address window for the display.
    pub fn set_address_window(
        &mut self,
        sx: u16,
        sy: u16,
        ex: u16,
        ey: u16,
    ) -> Result<(), Error<PinE>> {
        self.write_command(Instruction::CASET, &[])?;
        self.signal_data()?;
        self.write_word(sx + self.dx)?;
        self.write_word(ex + self.dx)?;
        self.write_command(Instruction::RASET, &[])?;
        self.signal_data()?;
        self.write_word(sy + self.dy)?;
        self.write_word(ey + self.dy)
    }

    pub fn set_offset(&mut self, dx: u16, dy: u16) {
        self.dx = dx;
        self.dy = dy;
    }
}
