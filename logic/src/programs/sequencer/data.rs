use embedded_sdmmc::{BlockDevice, TimeSource};
use heapless::String;
use serde::{Serialize, Deserialize};
use ufmt::uwrite;
use voice_lib::VoiceTrack;

use crate::{util::DiscreetUnwrap, stdlib::{StdlibErrorFileWrapper, Closed}};
use crate::stdlib::{FileSystem, StdlibError, DataFile, File};

#[derive(Serialize, Deserialize)]
pub(super) struct SequenceFile {
    seq_name: String<8>,
    track: VoiceTrack
}


impl SequenceFile {

    pub(crate) fn new(seq_name: &str, size: usize) -> Self {
        Self { seq_name: seq_name.into(), track: VoiceTrack::new(size) }
    }

    fn _load_data_file(&self) -> DataFile<Closed> {
        let mut tmp = String::<12>::new();
        uwrite!(tmp, "{}.seq", &self.seq_name as &str).duwrp();
        DataFile::new(&tmp)
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

    pub(crate) async fn save<D: BlockDevice, TS: TimeSource>(
        &self,
        fs: &mut FileSystem<D, TS>,
    ) -> Result<(), StdlibError<D>> {
        let df = self._load_data_file();
        match df.open_write(fs, true).await {
            Ok(mut f) => {
                f.dump(fs, self).await?;
                f.close(fs)?;
                Ok(())
            },
            Err(StdlibErrorFileWrapper(e, _)) => {
                Err(e)
            },
        }
    }

}
