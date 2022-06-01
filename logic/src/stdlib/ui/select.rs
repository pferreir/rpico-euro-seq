use core::ops::Rem;

use embedded_graphics::{draw_target::DrawTarget, Drawable};

pub(crate) trait Selectable {
    fn with_selected(self, selected: bool) -> Self;
}

pub(crate) struct SelectGroup<'t, const N: usize, DT: DrawTarget<Color = C>, C> {
    current: usize,
    dt: &'t mut DT,
    selected: isize
}

impl<'t, C, DT: DrawTarget<Color = C>, const N: usize> SelectGroup<'t, N, DT, C> {
    pub fn new(dt: &'t mut DT, selected: isize) -> Self {
        Self { current: 0, dt, selected }
    }

    pub fn add<D: Drawable<Color=C> + Selectable>(&mut self, d: D) -> Result<D, DT::Error> {
        let item = d.with_selected(self.selected.rem_euclid(N as isize) as usize == self.current);
        item.draw(self.dt)?;
        self.current += 1;
        Ok(item)
    }
}

impl<'t, C, DT: DrawTarget<Color = C>, const N: usize> Drop for SelectGroup<'t, N, DT, C> {
    fn drop(&mut self) {
        assert!(self.current == {N}, "N should be equal to the number of fields")
    }
}
