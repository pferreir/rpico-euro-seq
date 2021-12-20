use core::{
    cell::RefCell,
    ops::Deref,
    sync::atomic::{compiler_fence, Ordering},
};

use cortex_m::{delay::Delay, interrupt::free, interrupt::Mutex};
use display_interface::{DisplayError, WriteOnlyDataCommand};
use embedded_dma::{ReadBuffer, ReadTarget};
use embedded_hal::digital::v2::OutputPin;
use rp2040_hal::{
    dma::{Pace, SingleBufferingConfig, SingleChannel},
    spi::{Enabled, SpiDevice},
    Spi,
};

struct BufferWrapper<T: Sized + 'static>(&'static mut [T], usize);

pub const SPI_DEVICE_READY: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(true));

unsafe impl<T: ReadTarget<Word = u8>> ReadBuffer for BufferWrapper<T> {
    type Word = T::Word;

    unsafe fn read_buffer(&self) -> (*const Self::Word, usize) {
        (self.0.as_ptr() as *const Self::Word, self.1)
    }
}

pub struct DMASPIInterface<D: SpiDevice, DC: OutputPin, CH1: SingleChannel> {
    dc: DC,
    config_data: Option<(CH1, Spi<Enabled, D, 8>)>,
    tx_buffer: Option<BufferWrapper<u8>>,
}

impl<'t, D: SpiDevice, DC: OutputPin, CH1: SingleChannel> DMASPIInterface<D, DC, CH1> {
    pub fn new(ch: CH1, buffer: &'static mut [u8; 1024], spi: Spi<Enabled, D, 8>, dc: DC) -> Self {
        Self {
            tx_buffer: Some(BufferWrapper(buffer, 1024)),
            config_data: Some((ch, spi)),
            dc,
        }
    }
}

impl<D: SpiDevice, DC: OutputPin, CH1: SingleChannel> WriteOnlyDataCommand
    for DMASPIInterface<D, DC, CH1>
where
    Self: 'static,
{
    fn send_commands(&mut self, cmd: display_interface::DataFormat) -> Result<(), DisplayError> {
        // let (_, spi) = self.config_data.as_mut().unwrap();
        loop {
            let ready = free(|cs| *SPI_DEVICE_READY.borrow(cs).borrow());
            if ready {
                break;
            }
        }
        // while !spi.is_tx_fifo_empty() {}
        // while spi.is_busy() {}
        self.dc.set_low().map_err(|_| DisplayError::DCError)?;
        self.send_byte_buf(cmd);
        Ok(())
    }

    fn send_data(&mut self, buf: display_interface::DataFormat) -> Result<(), DisplayError> {
        // let (_, spi) = self.config_data.as_mut().unwrap();
        loop {
            let ready = free(|cs| *SPI_DEVICE_READY.borrow(cs).borrow());
            if ready {
                break;
            }
        }
        // while !spi.is_tx_fifo_empty() {}
        // while spi.is_busy() {}
        self.dc.set_high().map_err(|_| DisplayError::DCError)?;
        self.send_byte_buf(buf);

        Ok(())
    }
}

impl<D: SpiDevice + Deref, DC: OutputPin, CH1: SingleChannel> DMASPIInterface<D, DC, CH1>
where
    Self: 'static,
{
    fn _trigger_dma_transfer(&mut self, tx_buffer: BufferWrapper<u8>) -> BufferWrapper<u8> {
        let (ch, to) = self.config_data.take().unwrap();

        free(|cs| {
            let singleton = SPI_DEVICE_READY;
            let mut ready = singleton.borrow(cs).borrow_mut();
            *ready = false;
        });

        let mut config = SingleBufferingConfig::new(ch, tx_buffer, to);
        config.pace(Pace::PreferSink);
        let tx = config.start();

        let (ch, tx_buffer, to) = tx.wait();
        self.config_data.replace((ch, to));
        tx_buffer
    }

    fn _send_byte_buf(&mut self, iter: impl Iterator<Item = u8>) -> u32 {
        let mut counter = 0u32;
        let mut tx_buffer = self.tx_buffer.take().unwrap();

        for src in iter {
            tx_buffer.0[(counter % 1024) as usize] = src;
            counter += 1;

            if counter == 1024 {
                tx_buffer.1 = 1024;
                tx_buffer = self._trigger_dma_transfer(tx_buffer);
                counter = 0;
            }
        }

        if counter > 0 {
            tx_buffer.1 = counter as usize;
            tx_buffer = self._trigger_dma_transfer(tx_buffer);
        }
        self.tx_buffer.replace(tx_buffer);

        counter
    }

    fn send_byte_buf(&mut self, buf: display_interface::DataFormat) {
        // let (from, mut to) = self.config_data.take().unwrap();
        // send_u8(&mut to, buf).unwrap();
        // self.config_data.replace((from, to));

        match buf {
            display_interface::DataFormat::U8(slice) => self._send_byte_buf(slice.iter().cloned()),
            display_interface::DataFormat::U16(slice) => self._send_byte_buf(
                slice
                    .iter()
                    .map(|v| [(v & 0xff) as u8, (v >> 8) as u8])
                    .flatten(),
            ),
            display_interface::DataFormat::U16BE(slice) => self._send_byte_buf(
                slice
                    .iter()
                    .map(|v| u16::to_be(*v))
                    .map(|v| [(v & 0xff) as u8, (v >> 8) as u8])
                    .flatten(),
            ),
            display_interface::DataFormat::U16LE(slice) => self._send_byte_buf(
                slice
                    .iter()
                    .map(|v| u16::to_le(*v))
                    .map(|v| [(v & 0xff) as u8, (v >> 8) as u8])
                    .flatten(),
            ),
            display_interface::DataFormat::U8Iter(iter) => self._send_byte_buf(iter),
            display_interface::DataFormat::U16BEIter(iter) => self._send_byte_buf(
                iter.map(u16::to_be)
                    .map(|v| [(v & 0xff) as u8, (v >> 8) as u8])
                    .flatten(),
            ),
            display_interface::DataFormat::U16LEIter(iter) => self._send_byte_buf(
                iter.map(u16::to_le)
                    .map(|v| [(v >> 8) as u8, (v & 0xff) as u8])
                    .flatten(),
            ),
            _ => unimplemented!(),
        };
    }
}
