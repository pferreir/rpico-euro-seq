use core::convert::Infallible;

use embedded_graphics::{prelude::*, primitives::Rectangle, pixelcolor::{Rgb565, raw::RawU16}};
use logic::screen::{SCREEN_WIDTH, SCREEN_HEIGHT};

const DISPLAY_AREA: Rectangle = Rectangle::new(
    Point::zero(),
    Size::new(SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32),
);

pub struct Framebuffer {
    pub video_buffer: [u8; SCREEN_WIDTH * SCREEN_HEIGHT * 2],
}
impl Framebuffer {
    pub fn new() -> Self {
        Self {
            video_buffer: [0u8; SCREEN_HEIGHT * SCREEN_WIDTH * 2],
        }
    }

    pub fn draw_pixel(&mut self, point: Point, color: Rgb565) {
        if !DISPLAY_AREA.contains(point) {
            return;
        }
        let i = (point.x + point.y * SCREEN_WIDTH as i32) * 2;
        let color: RawU16 = color.into();
        self.video_buffer[i as usize] = (color.into_inner() >> 8) as u8;
        self.video_buffer[i as usize + 1] = (color.into_inner() & 0xff) as u8;
    }

    pub unsafe fn buffer_addr(&self) -> (u32, u32) {
        let ptr = self as *const _ as *const u8;
        (ptr as u32, self.video_buffer.len() as u32)
    }
}

impl DrawTarget for Framebuffer {
    type Color = Rgb565;

    type Error = Infallible;

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

impl OriginDimensions for Framebuffer {
    fn size(&self) -> Size {
        Size::new(SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32)
    }
}
