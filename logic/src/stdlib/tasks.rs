use core::fmt::Debug;

use alloc::{boxed::Box, format};
use embedded_sdmmc::{BlockDevice, TimeSource};
use heapless::String;

use crate::{
    log::{debug, error, info}, util::DiscreetUnwrap,
};
use futures::{StreamExt, Stream, Sink, SinkExt};

use super::{DataFile, File, FileSystem, FileType};

pub struct SignalId(pub u64);

pub enum TaskType {
    FileSave(FileType, String<12>, Box<[u8]>),
    FileLoad(FileType, String<12>)
}

pub struct Task(pub u32, pub TaskType);

pub type TaskId = u32;
pub type TaskReturn = (TaskId, TaskResult);

#[derive(Debug)]
pub enum TaskResult {
    Done,
    Error(String<32>)
}

pub struct TaskManager<B: BlockDevice, TS: TimeSource> {
    fs: FileSystem<B, TS>,
}

impl<'t, B: BlockDevice + 't, TS: TimeSource + 't> TaskManager<B, TS> {
    pub fn new(fs: FileSystem<B, TS>) -> Self {
        Self {
            fs,
        }
    }

    pub async fn run_tasks(&mut self, rx_channel: &mut (impl Stream<Item = Task> + Unpin), tx_channel: &mut (impl Sink<TaskReturn> + Unpin)) {
        info("Task process running");
        loop {
            if let Some(task) = rx_channel.next().await {
                debug(&format!("Running task {}", task.0));
                tx_channel.send((task.0, match task.1 {
                    TaskType::FileSave(file_type, file_name, data) => {
                        info("Saving file...");
                        let f = DataFile::new(&file_name);
                        let tr = match f.open_write(&mut self.fs, false).await {
                            Ok(mut f) => {
                                debug("Dumping bytes...");
                                match f.dump_bytes(&mut self.fs, &data).await {
                                    Ok(()) => {
                                        f.close(&mut self.fs).unwrap();
                                        TaskResult::Done
                                    }
                                    Err(e) => {
                                        let err_str = format!("Error writing: {:?}", e);
                                        error(&err_str);
                                        TaskResult::Error("Error writing file".into())
                                    }
                                }
                            }
                            Err(e) => {
                                error(&format!("Error opening: {:?}", e));
                                TaskResult::Error("Error writing file".into())
                            }
                        };
                        tr
                    }
                    TaskType::FileLoad(_, _) => todo!(),
                })).await.duwrp();
            }
        }
    }
}

pub trait TaskInterface {
    type Error: Debug;

    fn submit(&mut self, task_type: TaskType) -> Result<TaskId, Self::Error>;
    fn pop(&mut self) -> Result<Option<TaskReturn>, Self::Error>;
}
