mod framebuffer;
mod st7735;

use core::{
    cell::RefCell,
    future::Future,
    sync::atomic::{compiler_fence, Ordering},
    task::Poll,
};

use cortex_m::{
    delay::Delay,
};
use critical_section::{Mutex, with, CriticalSection};
use embassy_sync::waitqueue::AtomicWaker;
use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb565, prelude::*};
pub use framebuffer::Framebuffer;
use logic::screen::{SCREEN_HEIGHT, SCREEN_WIDTH};
use rp2040_hal::{
    gpio::{
        pin::{
            bank0::{Gpio13, Gpio14, Gpio15, Gpio18, Gpio19},
            FunctionSpi,
        },
        Output, Pin, PushPull,
    },
    pac::{self, Peripherals, SPI0},
    spi::{Enabled, SpiDevice},
    Spi,
};

use st7735::{Instruction, ST7735};

pub type ScreenDriverWithPins = ST7735<
    Spi<Enabled, SPI0, 8>,
    Pin<Gpio13, Output<PushPull>>,
    Pin<Gpio14, Output<PushPull>>,
    Pin<Gpio15, Output<PushPull>>,
>;

pub const SPI_DEVICE_READY: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(true));
pub const DMA_READY: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(true));

static DMA_WAKER: AtomicWaker = AtomicWaker::new();

pub struct PollFuture<F: Fn() -> bool> {
    f: F,
}
impl<F: Fn() -> bool> Future for PollFuture<F> {
    type Output = ();

    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> Poll<Self::Output> {
        if (self.f)() {
            Poll::Ready(())
        } else {
            DMA_WAKER.register(cx.waker());
            Poll::Pending
        }
    }
}

impl<F: Fn() -> bool> PollFuture<F> {
    pub fn new(f: F) -> Self {
        Self { f }
    }
}

pub fn init_screen<'t>(
    spi: Spi<Enabled, SPI0, 8>,
    delay: &mut Delay,
    (_clk, _mosi, rst, dc, cs): (
        Pin<Gpio18, FunctionSpi>,
        Pin<Gpio19, FunctionSpi>,
        Pin<Gpio14, Output<PushPull>>,
        Pin<Gpio13, Output<PushPull>>,
        Pin<Gpio15, Output<PushPull>>,
    ),
) -> (Framebuffer, ScreenDriverWithPins) {
    let mut driver = ST7735::new(
        spi,
        dc,
        Some(rst),
        cs,
        SCREEN_WIDTH as u16,
        SCREEN_HEIGHT as u16,
    );
    driver.init(delay).unwrap();
    driver
        .set_orientation(&st7735::Orientation::Landscape)
        .unwrap();

    let mut screen = Framebuffer::new();
    screen.clear(Rgb565::BLACK).unwrap();
    (screen, driver)
}

#[inline(never)]
fn config_dma<D: SpiDevice>(ch: &pac::dma::CH, src_buf: u32, len: u32, spi: &D) {
    let dest = &spi.sspdr as *const _ as u32;

    ch.ch_al1_ctrl.write(|w| unsafe {
        w.data_size()
            .bits(0) // 0x00 -> 1 byte
            .incr_read()
            .bit(true) // incr SRC (mem position)
            .incr_write()
            .bit(false) // do not incr DEST (peripheral)
            .treq_sel()
            .bits(16) // TREQ 16 = SPI0 TX
            .chain_to()
            .bits(0) // chain to itself (don't chain)
            .en()
            .bit(true) // enable
    });
    ch.ch_read_addr.write(|w| unsafe { w.bits(src_buf) });
    ch.ch_trans_count.write(|w| unsafe { w.bits(len) });
    ch.ch_al2_write_addr_trig.write(|w| unsafe { w.bits(dest) });

    cortex_m::asm::dsb();
    compiler_fence(Ordering::SeqCst);
}

#[inline(never)]
pub async fn trigger_dma_transfer<SPI: SpiDevice>(
    dma: &pac::DMA,
    chan_no: usize,
    spi: &SPI,
    (ptr, len): (u32, u32),
) {
    with(|cs| {
        let singleton = DMA_READY;
        let mut ready = singleton.borrow(cs).borrow_mut();
        *ready = false;
    });

    config_dma(&dma.ch[chan_no], ptr, len, spi);

    // trigger transfer
    // dma.multi_chan_trigger
    //    .write(|w| unsafe { w.bits(1 << chan_no) });

    // wait

    loop {
        let ready = with(|cs| {
            let singleton = DMA_READY;
            let ready = singleton.borrow(cs).borrow();
            *ready
        });
        if ready {
            break;
        }
    }

    // while dma.ch[chan_no].ch_al1_ctrl.read().busy().bit_is_set() {}

    // TODO: get rid of this
    PollFuture::new(|| !dma.ch[chan_no].ch_al1_ctrl.read().busy().bit_is_set()).await;

    cortex_m::asm::dsb();
    compiler_fence(Ordering::SeqCst);
}

pub async fn refresh<SPI: SpiDevice>(
    dma: &pac::DMA,
    spi: SPI,
    video_buf: (u32, u32),
    screen_driver: &mut ScreenDriverWithPins,
    delay: &mut cortex_m::delay::Delay,
) {
    with(|cs| {
        let singleton = SPI_DEVICE_READY;
        let mut ready = singleton.borrow(cs).borrow_mut();
        *ready = false;
    });

    screen_driver
        .set_address_window(0, 0, SCREEN_WIDTH as u16, SCREEN_HEIGHT as u16)
        .unwrap();
    screen_driver
        .write_command(Instruction::RAMWR, &[])
        .unwrap();

    // let vref: &[u8; 240 * 240 * 2] = unsafe {
    //     &*(video_buf.0 as *const [u8; 240 * 240 * 2])
    // };
    // screen_driver.write_data(vref).unwrap();

    // TODO: get rid of this?
    while spi.sspsr.read().bsy().bit_is_set() {}

    screen_driver.signal_data().unwrap();
    trigger_dma_transfer(dma, 0, &spi, video_buf).await;
}

pub fn init_interrupts(pac: &mut Peripherals) {
    pac.SPI0.sspimsc.modify(|_, w| w.txim().set_bit());
    pac.DMA.inte0.modify(|_, w| unsafe { w.bits(0x1) });
}

pub fn handle_spi_irq(cs: CriticalSection, pac: &mut Peripherals) {
    let singleton = SPI_DEVICE_READY;
    let mut ready = singleton.borrow(cs).borrow_mut();
    let reg = pac.SPI0.sspmis.read();
    if reg.txmis().bit_is_set() {
        *ready = true;
        pac.SPI0.sspimsc.modify(|_, w| w.txim().clear_bit());
    }
}

#[inline(never)]
pub fn handle_dma_irq(cs: CriticalSection, pac: &mut Peripherals) {
    let singleton = DMA_READY;
    let mut ready = singleton.borrow(cs).borrow_mut();

    DMA_WAKER.wake();

    if (pac.DMA.ints0.read().bits() & 0x1) > 0 {
        *ready = true;

        // acknowledge
        pac.DMA.ints0.modify(|_, w| unsafe { w.ints0().bits(0x1) })
    }
}
