use core::{
    cell::RefCell,
    convert::Infallible,
    marker::PhantomData,
    ops::Deref
};

use cortex_m::{
    delay::Delay,
    interrupt::{free, CriticalSection, Mutex},
    singleton,
};
use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::{
        raw::{RawU16, ToBytes},
        Rgb565,
    },
    prelude::{OriginDimensions, Point, RgbColor, Size},
    primitives::Rectangle,
    Pixel,
};
use embedded_hal::digital::v2::OutputPin;
use rp2040_hal::{
    dma::{Channel, SingleChannel, CH0},
    gpio::{
        pin::{
            bank0::{BankPinId, Gpio13, Gpio14, Gpio15, Gpio18, Gpio19},
            FunctionSpi,
        },
        Output, Pin, PinId, PushPull,
    },
    pac::{Peripherals, SPI0},
    spi::{Enabled, SpiDevice},
    Spi,
};

use st7789_dma::ST7789DMA;

mod st7789_dma;

pub type ScreenWithPins =
    Screen<SPI0, Channel<CH0>, Gpio18, Gpio19, Gpio14, Pin<Gpio13, Output<PushPull>>, Gpio15>;

pub const SPI_DEVICE_READY: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(true));

pub const DMA_BUFFER_SIZE: usize = 1024;
pub const SCREEN_WIDTH: usize = 240;
pub const SCREEN_HEIGHT: usize = 240;
const DISPLAY_AREA: Rectangle = Rectangle::new(
    Point::zero(),
    Size::new(SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32),
);

pub struct Screen<
    D: SpiDevice + Deref + 'static,
    CH: SingleChannel + 'static,
    CLK: PinId,
    MOSI: PinId,
    RST: PinId,
    DC: OutputPin + 'static,
    BLT: PinId,
> {
    _ch: PhantomData<CH>,
    _clk: PhantomData<CLK>,
    _mosi: PhantomData<MOSI>,
    _rst: PhantomData<RST>,
    _dc: PhantomData<DC>,
    _blt: PhantomData<BLT>,
    st7789: ST7789DMA<CH, D, Pin<RST, Output<PushPull>>, DC, Infallible>,
    video_buffer: &'static mut [Rgb565; SCREEN_WIDTH * SCREEN_HEIGHT],
}
impl<
        D: SpiDevice + Deref,
        CH: SingleChannel,
        CLK: PinId + BankPinId,
        MOSI: PinId + BankPinId,
        RST: PinId + BankPinId,
        DC: OutputPin<Error = Infallible>,
        BLT: PinId + BankPinId,
    > Screen<D, CH, CLK, MOSI, RST, DC, BLT>
{
    pub fn new(
        dma_buffer: &'static mut [u8; DMA_BUFFER_SIZE],
        ch: CH,
        spi: Spi<Enabled, D, 8>,
        _clk: Pin<CLK, FunctionSpi>,
        _mosi: Pin<MOSI, FunctionSpi>,
        dc: DC,
        rst: Pin<RST, Output<PushPull>>,
        mut blt: Pin<BLT, Output<PushPull>>,
        video_buffer: &'static mut [Rgb565; SCREEN_WIDTH * SCREEN_HEIGHT]
    ) -> Self {
        blt.set_high().unwrap();

        let st7789 = ST7789DMA::new(dma_buffer, ch, spi, rst, dc, 240, 240);
        Self {
            st7789,
            _ch: PhantomData,
            _clk: PhantomData,
            _mosi: PhantomData,
            _rst: PhantomData,
            _dc: PhantomData,
            _blt: PhantomData,
            video_buffer,
        }
    }

    pub fn init(&mut self, delay: &mut Delay) {
        self.st7789.init(delay).unwrap();
        self.st7789
            .set_orientation(st7789_dma::Orientation::Portrait)
            .unwrap();
        // self.st7789.clear(0x0).await.unwrap();
    }

    pub fn draw_pixel(&mut self, point: Point, color: Rgb565) {
        if !DISPLAY_AREA.contains(point) {
            return;
        }
        let i = point.x + point.y * SCREEN_WIDTH as i32;
        self.video_buffer[i as usize] = color;
    }

    pub fn refresh(&mut self) {
        self.st7789
            .set_pixels(
                0,
                0,
                SCREEN_WIDTH as u16,
                SCREEN_HEIGHT as u16,
                self.video_buffer.iter().cloned(),
            )
            .unwrap();
    }
}

impl<
        D: SpiDevice + Deref,
        CH: SingleChannel,
        CLK: PinId + BankPinId,
        MOSI: PinId + BankPinId,
        RST: PinId + BankPinId,
        DC: OutputPin<Error = Infallible>,
        BLT: PinId + BankPinId,
    > DrawTarget for Screen<D, CH, CLK, MOSI, RST, DC, BLT>
{
    type Color = Rgb565;

    type Error = st7789_dma::Error<Infallible>;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
    {
        for pixel in pixels.into_iter() {
            let Pixel(point, color) = pixel;

            self.draw_pixel(point, color);
        }

        Ok(())
    }
}

impl<
        D: SpiDevice + Deref,
        CH: SingleChannel,
        CLK: PinId + BankPinId,
        MOSI: PinId + BankPinId,
        RST: PinId + BankPinId,
        DC: OutputPin<Error = Infallible>,
        BLT: PinId + BankPinId,
    > OriginDimensions for Screen<D, CH, CLK, MOSI, RST, DC, BLT>
{
    fn size(&self) -> embedded_graphics::prelude::Size {
        self.st7789.size()
    }
}

pub fn init_screen(
    dma_buffer: &'static mut [u8; DMA_BUFFER_SIZE],
    ch: Channel<CH0>,
    spi: Spi<Enabled, SPI0, 8>,
    delay: &mut Delay,
    clk: Pin<Gpio18, FunctionSpi>,
    mosi: Pin<Gpio19, FunctionSpi>,
    rst: Pin<Gpio14, Output<PushPull>>,
    dc: Pin<Gpio13, Output<PushPull>>,
    blt: Pin<Gpio15, Output<PushPull>>,
    video_buffer: &'static mut [Rgb565; SCREEN_WIDTH * SCREEN_HEIGHT]
) -> ScreenWithPins {
    let mut screen = Screen::new(dma_buffer, ch, spi, clk, mosi, dc, rst, blt, video_buffer);
    screen.init(delay);
    screen.clear(Rgb565::BLUE).unwrap();
    screen
}

pub fn init_interrupts(pac: &mut Peripherals) {
    pac.SPI0.sspimsc.modify(|_, w| w.txim().set_bit());
}

pub fn handle_irq(cs: &CriticalSection, pac: &mut Peripherals) {
    let singleton = SPI_DEVICE_READY;
    let mut ready = singleton.borrow(cs).borrow_mut();
    let reg = pac.SPI0.sspmis.read();
    if reg.txmis().bit_is_set() {
        *ready = true;
    }
    pac.SPI0.sspimsc.modify(|_, w| w.txim().clear_bit());
}
