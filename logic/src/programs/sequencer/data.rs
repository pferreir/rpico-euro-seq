use alloc::boxed::Box;
use embedded_sdmmc::{BlockDevice, TimeSource};
use heapless::String;
use serde::{Serialize, Deserialize};
use ufmt::uwrite;
use ciborium::ser::into_writer;

use crate::{util::DiscreetUnwrap, stdlib::{StdlibErrorFileWrapper, Closed, TaskType}, log::{info, debug}};
use crate::stdlib::{FileSystem, StdlibError, DataFile, File};

const FILE_BUFFER_SIZE: usize = 10240;

#[derive(Serialize, Deserialize)]
pub(super) struct SequenceFile {
    seq_name: String<8>
}


impl SequenceFile {

    pub(crate) fn new(seq_name: &str) -> Self {
        Self { seq_name: seq_name.into() }
    }

    fn _load_data_file(&self) -> DataFile<Closed> {
        let mut tmp = String::<12>::new();
        uwrite!(tmp, "{}.seq", &self.seq_name as &str).duwrp();
        DataFile::new(&tmp)
    }

    pub(crate) fn set_name(&mut self, file_name: &str) {
        self.seq_name = file_name.into();
    }

    pub(crate) async fn load<D: BlockDevice, TS: TimeSource>(
        &self,
        fs: &mut FileSystem<D, TS>,
    ) -> Result<Self, StdlibError<D>> {
        let df = self._load_data_file();
        match df.open_read(fs).await {
            Ok(mut f) => {
                let seq_file = f.load(fs).await?;
                f.close(fs)?;
                Ok(seq_file)
            }
            Err(StdlibErrorFileWrapper(e, _)) => {
                Err(e)
            },
        }
    }

    pub(crate) fn save<D: BlockDevice>(
        &self,
    ) -> Result<TaskType, StdlibError<D>> {
        let mut buffer = [0u8; FILE_BUFFER_SIZE];
        debug("Serializing data...");
        into_writer(self, &mut buffer[..]).map_err(StdlibError::<D>::Serialization)?;
        debug("done");

        let mut file_name = String::<12>::new();
        uwrite!(file_name, "{}.seq", &self.seq_name as &str).duwrp();

        Ok(TaskType::FileSave(file_name, Box::new(buffer)))
    }

}
