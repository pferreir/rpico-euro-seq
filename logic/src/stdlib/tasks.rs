use alloc::{boxed::Box, collections::BTreeMap, format};
use embedded_sdmmc::{BlockDevice, TimeSource};
use heapless::String;

use crate::{
    log::{debug, error, info}, util::DiscreetUnwrap,
};
use futures::{channel::mpsc::{self, Receiver, Sender}, StreamExt};

use super::{DataFile, File, FileSystem};

pub struct SignalId(pub u64);

pub enum TaskType {
    FileSave(String<12>, Box<[u8]>),
}

pub struct Task(u32, TaskType);

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

    pub async fn run_tasks(&mut self, rx_channel: &mut mpsc::Receiver<Task>, tx_channel: &mut mpsc::Sender<TaskReturn>) {
        while let Some(task) = rx_channel.next().await {
            tx_channel.try_send((task.0, match task.1 {
                TaskType::FileSave(file_name, data) => {
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
                    }
                }
            })).duwrp()
        }
    }
}

pub struct TaskInterface {
    receiver: Receiver<TaskReturn>,
    sender: Sender<Task>,
    id_counter: u32
}

impl TaskInterface {
    pub fn new(receiver: Receiver<TaskReturn>, sender: Sender<Task>) -> Self {
        Self {
            receiver,
            sender,
            id_counter: 0
        }
    }

    pub fn submit(&mut self, task_type: TaskType) -> Result<(), mpsc::TrySendError<Task>>{
        self.sender.try_send(Task(self.id_counter, task_type))?;
        self.id_counter = self.id_counter.wrapping_add(1);
        Ok(())
    }

    pub fn receiver(&mut self) -> &mut Receiver<TaskReturn> {
        &mut self.receiver
    }
}
