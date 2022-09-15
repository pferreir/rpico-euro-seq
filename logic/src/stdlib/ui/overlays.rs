use alloc::{boxed::Box, vec::Vec, format};
use core::fmt::Debug;
use embedded_graphics::{pixelcolor::Rgb565, prelude::DrawTarget};
use embedded_sdmmc::{BlockDevice, TimeSource};

use crate::{
    programs::Program,
    stdlib::{SignalId, StdlibError, TaskInterface, TaskType},
    util::DiscreetUnwrap,
};

use super::UIInputEvent;

pub trait Overlay<
    't,
    D: DrawTarget<Color = Rgb565>,
    P: Program<'t, B, D, TS, TI>,
    B: BlockDevice + 't,
    TS: TimeSource + 't,
    TI: TaskInterface + 't,
>
{
    fn process_ui_input(&mut self, input: &UIInputEvent) -> OverlayResult<'t, D, P, B, TS, TI>
    where
        D: 't;

    fn run<'u>(
        &'u mut self,
    ) -> Result<
        Option<Box<dyn FnOnce(&mut P) -> Result<Vec<TaskType>, StdlibError> + 'u>>,
        StdlibError,
    >;
    fn draw(&self, target: &mut D) -> Result<(), D::Error>;
}

pub enum OverlayResult<
    't,
    D: DrawTarget<Color = Rgb565>,
    P: Program<'t, B, D, TS, TI>,
    B: BlockDevice + 't,
    TS: TimeSource + 't,
    TI: TaskInterface + 't,
> {
    Nop,
    Push(Box<dyn Overlay<'t, D, P, B, TS, TI> + 't>),
    Replace(Box<dyn Overlay<'t, D, P, B, TS, TI> + 't>),
    CloseOnSignal(SignalId),
    Close,
}

pub struct OverlayManager<
    't,
    P: Program<'t, B, D, TS, TI>,
    B: BlockDevice,
    TS: TimeSource,
    D: DrawTarget<Color = Rgb565>,
    TI: TaskInterface + 't,
> where
    D::Error: Debug,
{
    pub(crate) stack: Option<Vec<Box<dyn Overlay<'t, D, P, B, TS, TI> + 't>>>,
    pub(crate) pending_ops: Vec<OverlayResult<'t, D, P, B, TS, TI>>,
}

impl<
        't,
        P: Program<'t, B, D, TS, TI>,
        B: BlockDevice,
        TS: TimeSource,
        D: DrawTarget<Color = Rgb565> + 't,
        TI: TaskInterface,
    > OverlayManager<'t, P, B, TS, D, TI>
where
    D::Error: Debug,
{
    pub fn new() -> Self {
        Self {
            stack: Some(Vec::new()),
            pending_ops: Vec::new(),
        }
    }

    pub(crate) fn process_input(&mut self, msg: &UIInputEvent) -> Result<bool, StdlibError> {
        let mut overlays = self.stack.take().unwrap();
        let res = match overlays.last_mut() {
            Some(o) => {
                self.pending_ops.push(o.process_ui_input(msg));
                true
            }
            None => false,
        };
        self.stack.replace(overlays);
        Ok(res)
    }

    pub(crate) fn draw(&mut self, screen: &mut D) {
        let mut overlays = self.stack.take().unwrap();
        for overlay in overlays.iter_mut() {
            overlay.draw(screen).duwrp();
        }
        self.stack.replace(overlays);
    }

    pub(crate) fn run(&mut self, program: &mut P, task_iface: &mut TI) -> Result<(), StdlibError> {
        let mut overlays = self.stack.take().unwrap();

        for overlay in overlays.iter_mut() {
            match overlay.run()? {
                Some(f) => {
                    for submitted_task in f(program)? {
                        task_iface
                            .submit(submitted_task)
                            .map_err(|e| StdlibError::TaskInterface(format!("{:?}", e)))?;
                    }
                }
                None => {}
            }
        }

        for operation in self.pending_ops.drain(0..(self.pending_ops.len())) {
            match operation {
                OverlayResult::Nop => {}
                OverlayResult::Push(o) => {
                    overlays.push(o);
                }
                OverlayResult::Replace(o) => {
                    overlays.push(o);
                }
                OverlayResult::Close => {
                    overlays.pop();
                }
                OverlayResult::CloseOnSignal(_) => {}
            }
        }

        self.stack.replace(overlays);
        Ok(())
    }
}
