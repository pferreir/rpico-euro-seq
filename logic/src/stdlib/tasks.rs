use core::ops::DerefMut;

use alloc::{boxed::Box, collections::BTreeMap, format};
use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb565};
use embedded_sdmmc::{BlockDevice, TimeSource};
use heapless::String;

use crate::{log::{info, error, debug}, programs::Program};

use super::{FileSystem, DataFile, File};

pub struct SignalId(pub u64);

pub enum Task {
    FileSave(String<12>, Box<[u8]>),
}

pub struct TaskManager<B: BlockDevice, TS: TimeSource> {
    fs: FileSystem<B, TS>,
    tasks: BTreeMap<u64, Task>,
    signal_id: u64,
}

impl<'t, B: BlockDevice + 't, TS: TimeSource + 't> TaskManager<B, TS> {
    pub fn new(fs: FileSystem<B, TS>) -> Self {
        Self {
            tasks: BTreeMap::new(),
            signal_id: 0,
            fs,
        }
    }

    pub fn enqueue(&'t mut self, task: Task) -> SignalId {
        let id = self.signal_id;
        self.tasks.insert(id, task);
        self.signal_id += 1;
        SignalId(id)
    }

    pub async fn run_tasks<
        'u,
        D: DrawTarget<Color = Rgb565>,
        P: Program<'u, B, D, TS> + 'u,
        // PM: DerefMut<Target = P> + 'u,
    >(
        &mut self,
        program: &mut P,
    ) where
        B: 'u,
        TS: 'u,
    {
        for (tid, task) in self.tasks.iter_mut() {
            debug(&format!("running task {}", tid));
            match task {
                Task::FileSave(file_name, data) => {
                    info("SAVING FILE...");
                    let f = DataFile::new(file_name);
                    match f.open_write(&mut self.fs, false).await {
                        Ok(mut f) => {
                            match f.dump_bytes(&mut self.fs, data).await {
                                Ok(()) => {
                                    info("DONE");
                                    f.close(&mut self.fs).unwrap();
                                },
                                Err(e) => {
                                    error(&format!("{:?}", e));        
                                },
                            }
                        },
                        Err(e) => {
                            error(&format!("{:?}", e));
                        },
                    }
                    
                }
            }
        }

        self.tasks.clear();
    }
}
