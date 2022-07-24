use alloc::{boxed::Box, collections::BTreeMap, format};
use embedded_sdmmc::{BlockDevice, TimeSource};
use heapless::String;

use crate::{
    log::{debug, error, info},
};
use futures::{channel::mpsc, StreamExt};

use super::{DataFile, File, FileSystem};

pub struct SignalId(pub u64);

pub enum Task {
    FileSave(String<12>, Box<[u8]>),
}

pub enum TaskResult {
    Done
}

pub struct TaskManager<B: BlockDevice, TS: TimeSource> {
    fs: FileSystem<B, TS>,
    signal_id: u64,
}

impl<'t, B: BlockDevice + 't, TS: TimeSource + 't> TaskManager<B, TS> {
    pub fn new(fs: FileSystem<B, TS>) -> Self {
        Self {
            signal_id: 0,
            fs,
        }
    }

    pub async fn run_tasks(&mut self, rx_channel: &mut mpsc::Receiver<Task>, tx_channel: &mut mpsc::Sender<TaskResult>) {
        while let Some(task) = rx_channel.next().await {
            match task {
                Task::FileSave(file_name, data) => {
                    info("SAVING FILE...");
                    let f = DataFile::new(&file_name);
                    debug("Opening in write mode");
                    match f.open_write(&mut self.fs, false).await {
                        Ok(mut f) => {
                            debug("Dumping bytes...");
                            match f.dump_bytes(&mut self.fs, &data).await {
                                Ok(()) => {
                                    info("DONE");
                                    f.close(&mut self.fs).unwrap();
                                }
                                Err(e) => {
                                    error(&format!("Error writing: {:?}", e));
                                }
                            }
                        }
                        Err(e) => {
                            error(&format!("Error opening: {:?}", e));
                        }
                    }
                }
            }
        }
    }
}
