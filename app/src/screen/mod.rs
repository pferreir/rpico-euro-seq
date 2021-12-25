use core::{
    borrow::BorrowMut, cell::RefCell, convert::Infallible, marker::PhantomData, ops::Deref,
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
    prelude::{OriginDimensions, PixelColor, Point, RgbColor, Size},
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
    pac::{Peripherals, SPI0, clocks::clk_rtc_ctrl::W},
    spi::{Enabled, SpiDevice},
    Spi,
};
use st7789::{Error, ST7789};

use self::interface::{DMASPIInterface, SPI_DEVICE_READY};

mod interface;

const SCREEN_WIDTH: usize = 240;
const SCREEN_HEIGHT: usize = 240;
const DISPLAY_AREA: Rectangle = Rectangle::new(
    Point::zero(),
    Size::new(SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32),
);

pub static SCREEN: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(true));

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
    st7789: ST7789<DMASPIInterface<D, DC, CH>, Pin<RST, Output<PushPull>>>,
    video_buffer: [Rgb565; SCREEN_WIDTH * SCREEN_HEIGHT],
}
impl<
        D: SpiDevice + Deref,
        CH: SingleChannel,
        CLK: PinId + BankPinId,
        MOSI: PinId + BankPinId,
        RST: PinId + BankPinId,
        DC: OutputPin,
        BLT: PinId + BankPinId,
    > Screen<D, CH, CLK, MOSI, RST, DC, BLT>
{
    pub fn new(
        ch: CH,
        dma_buffer: &'static mut [u8; 1024],
        spi: Spi<Enabled, D, 8>,
        _clk: Pin<CLK, FunctionSpi>,
        _mosi: Pin<MOSI, FunctionSpi>,
        rst: Pin<RST, Output<PushPull>>,
        dc: DC,
        mut blt: Pin<BLT, Output<PushPull>>,
    ) -> Self {
        blt.set_high().unwrap();

        let di = DMASPIInterface::new(ch, dma_buffer, spi, dc);
        // let di = SPIInterfaceNoCS::new(spi, dc);
        let st7789: ST7789<_, Pin<RST, Output<PushPull>>> = ST7789::new(di, rst, 240, 240);
        Self {
            st7789,
            _ch: PhantomData,
            _clk: PhantomData,
            _mosi: PhantomData,
            _rst: PhantomData,
            _dc: PhantomData,
            _blt: PhantomData,
            video_buffer: [Rgb565::BLACK; SCREEN_WIDTH * SCREEN_HEIGHT],
        }
    }

    pub fn init(&mut self, delay: &mut Delay) {
        self.st7789.init(delay).unwrap();
        self.st7789
            .set_orientation(st7789::Orientation::Portrait)
            .unwrap();
        self.st7789.clear(Rgb565::BLUE).unwrap();
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
                self.video_buffer.iter().map(|c| {
                    let b = c.to_le_bytes();
                    ((b[1] as u16) << 8) | (b[0] as u16)
                }),
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
        DC: OutputPin,
        BLT: PinId + BankPinId,
    > DrawTarget for Screen<D, CH, CLK, MOSI, RST, DC, BLT>
{
    type Color = Rgb565;

    type Error = Error<Infallible>;

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
        DC: OutputPin,
        BLT: PinId + BankPinId,
    > OriginDimensions for Screen<D, CH, CLK, MOSI, RST, DC, BLT>
{
    fn size(&self) -> embedded_graphics::prelude::Size {
        self.st7789.size()
    }
}

pub fn init_screen(
    ch: Channel<CH0>,
    spi: Spi<Enabled, SPI0, 8>,
    delay: &mut Delay,
    clk: Pin<Gpio18, FunctionSpi>,
    mosi: Pin<Gpio19, FunctionSpi>,
    rst: Pin<Gpio14, Output<PushPull>>,
    dc: Pin<Gpio13, Output<PushPull>>,
    blt: Pin<Gpio15, Output<PushPull>>,
) -> Screen<SPI0, Channel<CH0>, Gpio18, Gpio19, Gpio14, Pin<Gpio13, Output<PushPull>>, Gpio15> {
    let dma_buffer = singleton!(: [u8; 1024] = [0; 1024]).unwrap();
    let mut screen = Screen::new(ch, dma_buffer, spi, clk, mosi, rst, dc, blt);
    screen.init(delay);
    screen
}

fn with_singleton<S: DrawTarget<Color = Rgb565, Error = Error<Infallible>>, F: Fn(&mut S)>(
    s: &Mutex<RefCell<Option<S>>>,
    f: F,
) {
    free(|cs| {
        let mut singleton = s.borrow(cs).borrow_mut();
        match singleton.as_mut() {
            Some(s) => f(s),
            None => {
                panic!("Screen object not available!");
            }
        }
    });
}

pub fn init_interrupts(pac: &mut Peripherals) {
    pac.SPI0.sspimsc.modify(|_, w| {
        w.txim().set_bit()
    });
}

pub fn handle_irq(cs: &CriticalSection, pac: &mut Peripherals) {
    let singleton = SPI_DEVICE_READY;
    let mut ready = singleton.borrow(cs).borrow_mut();
    let reg = pac.SPI0.sspmis.read();
    if reg.txmis().bit_is_set() {
        *ready = true;
    }
    pac.SPI0.sspimsc.modify(|_, w| {
        w.txim().clear_bit()
    });
}
