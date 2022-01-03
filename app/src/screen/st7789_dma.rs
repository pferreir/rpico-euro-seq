use core::marker::PhantomData;
use core::ops::Deref;

use cortex_m::interrupt::free;
use embedded_dma::{ReadBuffer, ReadTarget};
use embedded_graphics::pixelcolor::raw::RawU16;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::{OriginDimensions, Point, RawData, Size};
use embedded_graphics::primitives::Rectangle;
use embedded_hal::blocking::spi::WriteIter;
use embedded_hal::blocking::{delay::DelayUs, spi::Write};
use embedded_hal::digital::v2::OutputPin;
use rp2040_hal::dma::{SingleBufferingConfig, SingleChannel};
use rp2040_hal::spi::{Enabled, SpiDevice};
use rp2040_hal::Spi;

use super::{DMA_BUFFER_SIZE, SPI_DEVICE_READY};

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

///
/// Tearing effect output setting.
///
#[derive(Copy, Clone)]
pub enum TearingEffect {
    /// Disable output.
    Off,
    /// Output vertical blanking information.
    Vertical,
    /// Output horizontal and vertical blanking information.
    HorizontalAndVertical,
}

#[derive(Debug)]
pub enum Error<PinE> {
    DMAError,
    DisplayError,
    Pin(PinE),
}

/// ST7789 instructions.
#[repr(u8)]
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
    VSCRDER = 0x33,
    TEOFF = 0x34,
    TEON = 0x35,
    MADCTL = 0x36,
    VSCAD = 0x37,
    COLMOD = 0x3A,
    VCMOFSET = 0xC5,
}

struct BufferWrapper<T: Sized + 'static>(&'static mut [T], usize);

impl<T: Sized + 'static> BufferWrapper<T> {
    pub fn new(buf: &'static mut [T], length: usize) -> Self {
        Self(buf, length)
    }
}

unsafe impl<T: ReadTarget<Word = u8>> ReadBuffer for BufferWrapper<T> {
    type Word = T::Word;

    unsafe fn read_buffer(&self) -> (*const Self::Word, usize) {
        (self.0.as_ptr() as *const Self::Word, self.1)
    }
}

// struct DMAFuture;
// impl Future for DMAFuture {
//     type Output = ();
//     fn poll(self: RustPin<&mut Self>, _cx: &mut Context) -> Poll<Self::Output> {
//         if free(|cs| *SPI_DEVICE_READY.borrow(cs).borrow()) {
//             Poll::Ready(())
//         } else {
//             Poll::Pending
//         }
//     }
// }

pub struct ST7789DMA<CH, SPI: SpiDevice + Deref, RST, DC, PinE> {
    endpoints: Option<(CH, Spi<Enabled, SPI, 8>)>,
    buffer: Option<BufferWrapper<u8>>,
    rst: RST,
    dc: DC,
    size_x: u32,
    size_y: u32,
    orientation: Orientation,
    _pine: PhantomData<PinE>,
}

impl<
        CH: SingleChannel,
        SPI: SpiDevice + Deref,
        RST: OutputPin<Error = PinE>,
        DC: OutputPin<Error = PinE>,
        PinE,
    > ST7789DMA<CH, SPI, RST, DC, PinE>
{
    pub fn new(
        dma_buffer: &'static mut [u8; DMA_BUFFER_SIZE],
        ch: CH,
        spi: Spi<Enabled, SPI, 8>,
        rst: RST,
        dc: DC,
        width: u32,
        height: u32,
    ) -> Self {
        Self {
            endpoints: Some((ch, spi)),
            buffer: Some(BufferWrapper::new(dma_buffer, DMA_BUFFER_SIZE)),
            rst,
            dc,
            size_x: width,
            size_y: height,
            orientation: Orientation::default(),
            _pine: PhantomData,
        }
    }

    ///
    /// Runs commands to initialize the display
    ///
    /// # Arguments
    ///
    /// * `delay_source` - mutable reference to a delay provider
    ///
    pub fn init(&mut self, delay_source: &mut impl DelayUs<u32>) -> Result<(), Error<PinE>> {
        self.hard_reset(delay_source)?;
        self.write_command(Instruction::SWRESET)?; // reset display
        delay_source.delay_us(150_000);
        self.write_command(Instruction::SLPOUT)?; // turn off sleep
        delay_source.delay_us(10_000);
        self.write_command(Instruction::INVOFF)?; // turn off invert
        self.write_command(Instruction::VSCRDER)?; // vertical scroll definition
        self.write_data(&[0u8, 0u8, 0x14u8, 0u8, 0u8, 0u8])?; // 0 TSA, 320 VSA, 0 BSA
        self.write_command(Instruction::MADCTL)?; // left -> right, bottom -> top RGB
        self.write_data(&[0b0000_0000])?;
        self.write_command(Instruction::COLMOD)?; // 16bit 65k colors
        self.write_data(&[0b0101_0101])?;
        self.write_command(Instruction::INVON)?; // hack?
        delay_source.delay_us(10_000);
        self.write_command(Instruction::NORON)?; // turn on display
        delay_source.delay_us(10_000);
        self.write_command(Instruction::DISPON)?; // turn on display
        delay_source.delay_us(10_000);
        Ok(())
    }

    ///
    /// Performs a hard reset using the RST pin sequence
    ///
    /// # Arguments
    ///
    /// * `delay_source` - mutable reference to a delay provider
    ///
    pub fn hard_reset(&mut self, delay_source: &mut impl DelayUs<u32>) -> Result<(), Error<PinE>> {
        self.rst.set_high().map_err(Error::Pin)?;
        delay_source.delay_us(10); // ensure the pin change will get registered
        self.rst.set_low().map_err(Error::Pin)?;
        delay_source.delay_us(10); // ensure the pin change will get registered
        self.rst.set_high().map_err(Error::Pin)?;
        delay_source.delay_us(10); // ensure the pin change will get registered

        Ok(())
    }

    ///
    /// Returns currently set orientation
    ///
    pub fn orientation(&self) -> Orientation {
        self.orientation
    }

    ///
    /// Sets display orientation
    ///
    pub fn set_orientation(&mut self, orientation: Orientation) -> Result<(), Error<PinE>> {
        self.write_command(Instruction::MADCTL)?;
        self.write_data(&[orientation as u8])?;
        self.orientation = orientation;
        Ok(())
    }

    pub fn clear(&mut self, color: Rgb565) -> Result<(), Error<PinE>>
    where
        Self: Sized,
    {
        let colors = core::iter::repeat(color).take(240 * 320); // blank entire HW RAM contents

        match self.orientation {
            Orientation::Portrait | Orientation::PortraitSwapped => {
                self.set_pixels(0, 0, 239, 319, colors)
            }
            Orientation::Landscape | Orientation::LandscapeSwapped => {
                self.set_pixels(0, 0, 319, 239, colors)
            }
        }
    }

    ///
    /// Sets a pixel color at the given coords.
    ///
    /// # Arguments
    ///
    /// * `x` - x coordinate
    /// * `y` - y coordinate
    /// * `color` - the Rgb565 color value
    ///
    pub async fn set_pixel(&mut self, x: u16, y: u16, color: Rgb565) -> Result<(), Error<PinE>> {
        self.set_address_window(x, y, x, y)?;
        self.write_command(Instruction::RAMWR)?;
        loop {
            let ready = free(|cs| *SPI_DEVICE_READY.borrow(cs).borrow());
            if ready {
                break;
            }
        }
        self.dc.set_high().map_err(Error::Pin)?;
        self._write_bytes_dma(core::iter::once(color))
            .map_err(|_| Error::DisplayError)
        // self._write_pixels_blocking(core::iter::once(color))
    }

    ///
    /// Sets pixel colors in given rectangle bounds.
    ///
    /// # Arguments
    ///
    /// * `sx` - x coordinate start
    /// * `sy` - y coordinate start
    /// * `ex` - x coordinate end
    /// * `ey` - y coordinate end
    /// * `colors` - anything that can provide `IntoIterator<Item = u16>` to iterate over pixel data
    ///
    pub fn set_pixels<T>(
        &mut self,
        sx: u16,
        sy: u16,
        ex: u16,
        ey: u16,
        colors: T,
    ) -> Result<(), Error<PinE>>
    where
        T: IntoIterator<Item = Rgb565>,
    {
        self.set_address_window(sx, sy, ex, ey)?;
        self.write_command(Instruction::RAMWR)?;
        loop {
            let ready = free(|cs| *SPI_DEVICE_READY.borrow(cs).borrow());
            if ready {
                break;
            }
        }
        self.dc.set_high().map_err(Error::Pin)?;
        self._write_bytes_dma(colors.into_iter())
        // self._write_pixels_blocking(colors)
    }

    ///
    /// Sets scroll offset "shifting" the displayed picture
    /// # Arguments
    ///
    /// * `offset` - scroll offset in pixels
    ///
    pub fn set_scroll_offset(&mut self, offset: u16) -> Result<(), Error<PinE>> {
        self.write_command(Instruction::VSCAD)?;
        self.write_data(&offset.to_be_bytes())
    }

    ///
    /// Release resources allocated to this driver back.
    /// This returns the display interface and the RST pin deconstructing the driver.
    ///
    pub fn release(
        self,
    ) -> (
        Option<(CH, Spi<Enabled, SPI, 8>)>,
        Option<BufferWrapper<u8>>,
        RST,
        DC,
    ) {
        (self.endpoints, self.buffer, self.rst, self.dc)
    }

    fn write_command(&mut self, command: Instruction) -> Result<(), Error<PinE>> {
        loop {
            let ready = free(|cs| *SPI_DEVICE_READY.borrow(cs).borrow());
            if ready {
                break;
            }
        }
        self.dc.set_low().map_err(Error::Pin)?;
        self._write_bytes_blocking(&[command as u8])
    }

    fn write_data(&mut self, data: &[u8]) -> Result<(), Error<PinE>> {
        loop {
            let ready = free(|cs| *SPI_DEVICE_READY.borrow(cs).borrow());
            if ready {
                break;
            }
        }
        self.dc.set_high().map_err(Error::Pin)?;
        self._write_bytes_blocking(data)
    }

    // Sets the address window for the display.
    fn set_address_window(
        &mut self,
        sx: u16,
        sy: u16,
        ex: u16,
        ey: u16,
    ) -> Result<(), Error<PinE>> {
        self.write_command(Instruction::CASET)?;
        self.write_data(&sx.to_be_bytes())?;
        self.write_data(&ex.to_be_bytes())?;
        self.write_command(Instruction::RASET)?;
        self.write_data(&sy.to_be_bytes())?;
        self.write_data(&ey.to_be_bytes())
    }

    ///
    /// Configures the tearing effect output.
    ///
    pub fn set_tearing_effect(&mut self, tearing_effect: TearingEffect) -> Result<(), Error<PinE>> {
        match tearing_effect {
            TearingEffect::Off => self.write_command(Instruction::TEOFF),
            TearingEffect::Vertical => {
                self.write_command(Instruction::TEON)?;
                self.write_data(&[0])
            }
            TearingEffect::HorizontalAndVertical => {
                self.write_command(Instruction::TEON)?;
                self.write_data(&[1])
            }
        }
    }

    fn _write_bytes_blocking(&mut self, data: &[u8]) -> Result<(), Error<PinE>> {
        let (_, spi) = self.endpoints.as_mut().unwrap();
        spi.write(data).map_err(|_| Error::DisplayError)
    }

    fn _write_pixels_blocking(
        &mut self,
        data: impl IntoIterator<Item = Rgb565>,
    ) -> Result<(), Error<PinE>> {
        let (_, spi) = self.endpoints.as_mut().unwrap();
        spi.write_iter(
            data.into_iter()
                .flat_map(|c| u16::to_le_bytes(RawU16::from(c).into_inner())),
        )
        .map_err(|_| Error::DisplayError)
    }

    fn _trigger_dma_transfer(&mut self, buffer: BufferWrapper<u8>) -> BufferWrapper<u8> {
        let (ch, spi) = self.endpoints.take().unwrap();
        free(|cs| {
            let singleton = SPI_DEVICE_READY;
            let mut ready = singleton.borrow(cs).borrow_mut();
            *ready = false;
        });

        let config = SingleBufferingConfig::new(ch, buffer, spi);
        let tx = config.start();

        let (ch, buffer, spi) = tx.wait();
        self.endpoints.replace((ch, spi));

        buffer
        // DMAFuture.await;
        // tx.release()
    }

    fn _write_bytes_dma(&mut self, data: impl Iterator<Item = Rgb565>) -> Result<(), Error<PinE>> {
        let mut buffer = self.buffer.take().unwrap();
        let mut counter = 0u32;

        for src in data
            .flat_map(|c| u16::to_le_bytes(RawU16::from(c).into_inner()))
        {
            buffer.0[counter as usize] = src;
            counter += 1;

            if counter == DMA_BUFFER_SIZE as u32 {
                buffer.1 = DMA_BUFFER_SIZE;
                buffer = self._trigger_dma_transfer(buffer);
                counter = 0;
            }
        }

        if counter > 0 {
            buffer.1 = counter as usize;
            buffer = self._trigger_dma_transfer(buffer);
        }

        self.buffer.replace(buffer);

        Ok(())
    }
}

impl<CH, SPI: SpiDevice, RST: OutputPin<Error = PinE>, DC: OutputPin, PinE>
    ST7789DMA<CH, SPI, RST, DC, PinE>
{
    /// Returns the bounding box for the entire framebuffer.
    fn framebuffer_bounding_box(&self) -> Rectangle {
        let size = match self.orientation {
            Orientation::Portrait | Orientation::PortraitSwapped => Size::new(240, 320),
            Orientation::Landscape | Orientation::LandscapeSwapped => Size::new(320, 240),
        };

        Rectangle::new(Point::zero(), size)
    }
}

impl<CH, SPI: SpiDevice, RST: OutputPin<Error = PinE>, DC: OutputPin, PinE> OriginDimensions
    for ST7789DMA<CH, SPI, RST, DC, PinE>
{
    fn size(&self) -> Size {
        Size::new(self.size_x, self.size_y) // visible area, not RAM-pixel size
    }
}
