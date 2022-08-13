use core::{cell::RefCell, mem::MaybeUninit};
use tinybmp::Bmp;
use embedded_graphics::pixelcolor::Rgb565;

pub struct Icon<'t> {
    inner: MaybeUninit<Bmp<'t, Rgb565>>,
    loaded: bool,
    data: &'static [u8]
}

unsafe impl<'t> Sync for Icon<'t> {}

impl<'t> Icon<'t> {
    pub const fn new(data: &'static [u8]) -> Self {
        Self {
            inner: MaybeUninit::uninit(),
            loaded: false,
            data
        }
    }

    fn load(&mut self) {
        self.loaded = true;
        self.inner.write(Bmp::from_slice(self.data).unwrap());
    }

    pub unsafe fn as_bmp(&mut self) -> &Bmp<'t, Rgb565> {
        if !self.loaded {
            self.load();
        }
        self.inner.assume_init_ref()
    }
}

#[macro_export]
macro_rules! decl_icon {
    ($name: ident, $path: literal) => {
        pub fn $name<'t>() -> &'t Bmp<'t, Rgb565> {
            static mut $name: Icon = Icon::new(include_bytes!($path));
            unsafe { $name.as_bmp() }
        }
    };
}

