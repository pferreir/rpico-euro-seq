use crate::{stdlib::{
    Closed, ConfigFile, DataFile, File, FileSystem, OpenWrite, StdlibError, StdlibErrorFileWrapper, FSError,
}, log};
use embedded_sdmmc::{BlockDevice, TimeSource};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub(super) struct Config {
    current_data_file: Option<DataFile<Closed>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            current_data_file: None,
        }
    }
}

impl Config {
    pub(crate) async fn load<D: BlockDevice, TS: TimeSource>(
        fs: &mut FileSystem<D, TS>,
    ) -> Result<Self, StdlibError> {
        let cf = ConfigFile::new("config.rmp");

        Ok(match cf.open_read(fs).await {
            Ok(mut f) => {
                log::info("Found config file");
                let config = f.load(fs).await?;
                f.close(fs)?;
                config
            }
            Err(StdlibErrorFileWrapper(StdlibError::FS(FSError::FileNotFound), f)) => {
                log::info("Creating new config file");
                let config = Config::default();
                let mut f: ConfigFile<OpenWrite> = f
                    .open_write(fs, true)
                    .await
                    .map_err(|StdlibErrorFileWrapper(e, _)| e)?;

                // write contents
                f.dump(fs, &config).await?;
                f.close(fs)?;
                config
            }
            Err(StdlibErrorFileWrapper(e, _)) => {
                return Err(e);
            }
        })
    }
}
