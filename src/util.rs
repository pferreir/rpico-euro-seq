use core::marker::PhantomData;

use heapless::spsc::Queue;

pub struct QueuePoppingIter<'t, T, const N: usize> {
    wrapped: &'t mut Queue<T, N>}

impl<'t, T, const N: usize> QueuePoppingIter<'t, T, N> {
    pub fn new(wrapped: &'t mut Queue<T, N>) -> Self {
        Self {
            wrapped
        }
    }
}

impl<'t, T, const N: usize> Iterator for QueuePoppingIter<'t, T, N> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.wrapped.dequeue()
    }

}
