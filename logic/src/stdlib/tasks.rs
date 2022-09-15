use core::fmt::Debug;

use alloc::{boxed::Box, format, vec::Vec};
use ciborium::value::Value;
use embedded_sdmmc::{BlockDevice, TimeSource};
use heapless::String;

use crate::{
    log::{debug, error, info}, util::DiscreetUnwrap,
};
use futures::{StreamExt, Stream, Sink, SinkExt};

use super::{FileSystem, File, FileContent, Closed, StdlibError, StdlibErrorFileWrapper};

pub struct SignalId(pub u64);

pub enum TaskType {
    FileSave(String<8>, String<12>, Box<dyn FileContent>),
    FileLoad(String<8>, String<12>),
    DirList(String<8>)
}

pub struct Task(pub u32, pub TaskType);

pub type TaskId = u32;
pub type TaskReturn = (TaskId, TaskResult);

#[derive(Debug)]
pub enum TaskResult {
    Done,
    FileContent(Value),
    DirList(Vec<File<Closed>>),
    Error(StdlibError)
}

pub struct TaskManager<B: BlockDevice, TS: TimeSource> {
    fs: FileSystem<B, TS>,
}



async fn save_file<B: BlockDevice, TS: TimeSource, S: FileContent + ?Sized>(fs: &mut FileSystem<B, TS>, dir: &str, file_name: &str, data: &S) -> Result<TaskResult, StdlibError> {
    let f = File::new(dir, file_name);
    info("Saving file...");
    let mut f = f.open_write(fs, false).await.map_err(|StdlibErrorFileWrapper(e, _)| e)?;
    debug("Dumping bytes...");
    f.dump(fs, &*data).await?;
    f.close(fs).unwrap();
    Ok(TaskResult::Done)
}

async fn load_file<B: BlockDevice, TS: TimeSource>(fs: &mut FileSystem<B, TS>, dir: &str, file_name: &str) -> Result<TaskResult, StdlibError> {
    let f = File::new(dir, file_name);
    info("Loading file...");
    let mut f = f.open_read(fs).await.map_err(|StdlibErrorFileWrapper(e, _)| e)?;
    debug("Reading bytes...");
    let content = f.load(fs).await?;
    f.close(fs).unwrap();
    Ok(TaskResult::FileContent(content))
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
                let result = match task.1 {
                    TaskType::FileSave(dir_name, file_name, data) =>  save_file(&mut self.fs, &dir_name, &file_name, &*data).await,
                    TaskType::FileLoad(dir_name, file_name) => load_file(&mut self.fs, &dir_name, &file_name).await,
                    TaskType::DirList(dir_name) => self.fs.list_files(&dir_name).await.map(|res| TaskResult::DirList(res)),
                };

                match result {
                    Ok(res) => {
                        tx_channel.send((task.0, res)).await.duwrp();
                    },
                    Err(e) => {
                        let err_str = format!("Error executing {:?}: {:?}", task.0, e);
                        error(&err_str);
                        tx_channel.send((task.0, TaskResult::Error(e))).await.duwrp();
                    }
                }
            }

        }
    }
}

pub trait TaskInterface {
    type Error: Debug;

    fn submit(&mut self, task_type: TaskType) -> Result<TaskId, Self::Error>;
    fn pop(&mut self) -> Result<Option<TaskReturn>, Self::Error>;
}
