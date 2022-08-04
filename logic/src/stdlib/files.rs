use ciborium::{ser::into_writer, de::from_reader};
use core::{marker::PhantomData, str, fmt::Debug};
use embedded_sdmmc::{
    BlockDevice, Controller, Directory, File as FATFile, Mode, ShortFileName, TimeSource, Volume,
    VolumeIdx,
};
use heapless::{String, Vec};
use serde::{self, de::DeserializeOwned, Deserialize, Serialize};
use ufmt::{uDisplay, uWrite, uwrite, Formatter};

use super::{StdlibError, StdlibErrorFileWrapper};

struct FileNameWrapper<'a>(&'a ShortFileName);

pub trait FileState {}

pub struct OpenRead;
impl FileState for OpenRead {}

pub struct OpenWrite;
impl FileState for OpenWrite {}

pub struct Closed;
impl FileState for Closed {}

pub trait File<S: FileState>: Debug {
    fn new(file_name: &str) -> Self;
    fn init_read(handle: Option<FATFile>, file_name: &str) -> Self;
    fn init_write(handle: Option<FATFile>, file_name: &str) -> Self;
    fn file_name(&self) -> &String<12>;
    fn handle_mut(&mut self) -> Option<&mut FATFile>;
}

impl<'a> uDisplay for FileNameWrapper<'a> {
    fn fmt<W>(&self, fmt: &mut Formatter<W>) -> Result<(), W::Error>
    where
        W: uWrite + ?Sized,
    {
        let base = str::from_utf8(self.0.base_name()).unwrap();
        let ext = str::from_utf8(self.0.extension()).unwrap();

        if ext.len() > 0 {
            uwrite!(fmt, "{}.{}", base, ext)?;
        } else {
            uwrite!(fmt, "{}", base)?;
        }
        Ok(())
    }
}

pub struct FileSystem<D: BlockDevice, TS: TimeSource> {
    controller: Controller<D, TS>,
    volume: Volume,
    data_dir: Directory,
    bin_dir: Directory,
    cfg_dir: Directory,
}

const FILE_BUFFER_SIZE: usize = 4096; // 4KB

async fn list_files<D: BlockDevice, TS: TimeSource, T: File<Closed>>(
    controller: &mut Controller<D, TS>,
    volume: &Volume,
    dir: &Directory,
    page: u32,
) -> Result<Vec<T, 16>, StdlibError> {
    let mut res = Vec::new();
    let start = page * 16;
    let end = (page + 1) * 16;
    let mut idx = 0;
    controller
        .iterate_dir(&volume, dir, |e| {
            if idx >= start && idx < end {
                let mut text = String::<12>::new();
                uwrite!(text, "{}", FileNameWrapper(&e.name)).unwrap();
                // this is basically infallible (unless, I f*ed up, which is not that unlikely)
                res.push(T::new(&text)).ok().unwrap();
            }
            idx += 1
        })
        .await;
    Ok(res)
}

impl<D: BlockDevice, TS: TimeSource> FileSystem<D, TS> {
    pub async fn new(block_device: D, timesource: TS) -> Result<FileSystem<D, TS>, StdlibError> {
        let mut controller = Controller::new(block_device, timesource);
        let volume = controller
            .get_volume(VolumeIdx(0))
            .await?;
        let root_dir = controller
            .open_root_dir(&volume)?;
        let cfg_dir = controller
            .open_dir(&volume, &root_dir, "cfg")
            .await?;
        let data_dir = controller
            .open_dir(&volume, &root_dir, "data")
            .await?;
        let bin_dir = controller
            .open_dir(&volume, &root_dir, "bin")
            .await?;
        Ok(Self {
            controller,
            volume,
            data_dir,
            bin_dir,
            cfg_dir,
        })
    }

    pub async fn list_data_files<'t>(
        &'t mut self,
        page: u32,
    ) -> Result<Vec<DataFile<Closed>, 16>, StdlibError> {
        list_files::<D, TS, DataFile<Closed>>(
            &mut self.controller,
            &self.volume,
            &self.data_dir,
            page,
        )
        .await
    }

    pub async fn list_bin_files<'t>(
        &'t mut self,
        page: u32,
    ) -> Result<Vec<BinFile<Closed>, 16>, StdlibError> {
        list_files::<D, TS, BinFile<Closed>>(
            &mut self.controller,
            &self.volume,
            &self.bin_dir,
            page,
        )
        .await
    }

    pub async fn list_cfg_files<'t>(
        &'t mut self,
        page: u32,
    ) -> Result<Vec<ConfigFile<Closed>, 16>, StdlibError> {
        list_files::<D, TS, ConfigFile<Closed>>(
            &mut self.controller,
            &self.volume,
            &self.cfg_dir,
            page,
        )
        .await
    }
}

async fn open_file<D: BlockDevice, TS: TimeSource, F: File<Closed>>(
    controller: &mut Controller<D, TS>,
    volume: &mut Volume,
    file: F,
    dir: &Directory,
    mode: Mode,
) -> Result<FATFile, StdlibErrorFileWrapper<F>> {
    controller
        .open_file_in_dir(volume, dir, &file.file_name(), mode)
        .await
        .map_err(|e| StdlibErrorFileWrapper(e.into(), file))
}

macro_rules! file_dir {
    ($fs: ident, DataFile) => {
        &$fs.data_dir
    };
    ($fs: ident, ConfigFile) => {
        &$fs.cfg_dir
    };
    ($fs: ident, BinFile) => {
        &$fs.bin_dir
    };
}

macro_rules! file_impl {
    ($s: ident) => {
        #[derive(Serialize, Deserialize)]
        pub struct $s<S: FileState> {
            pub file_name: String<12>,
            #[serde(skip)]
            pub handle: Option<FATFile>,

            _s: PhantomData<S>,
        }

        impl<S: FileState> uDisplay for $s<S> {
            fn fmt<W>(&self, formatter: &mut Formatter<'_, W>) -> Result<(), W::Error>
            where
                W: uWrite + ?Sized,
            {
                formatter.write_str(&self.file_name)
            }
        }

        impl<S: FileState> File<S> for $s<S> {
            fn new(file_name: &str) -> $s<S> {
                $s {
                    file_name: file_name.into(),
                    handle: None,
                    _s: PhantomData,
                }
            }

            fn init_read(handle: Option<FATFile>, file_name: &str) -> $s<S> {
                $s {
                    file_name: file_name.into(),
                    handle,
                    _s: PhantomData,
                }
            }

            fn init_write(handle: Option<FATFile>, file_name: &str) -> $s<S> {
                $s {
                    file_name: file_name.into(),
                    handle,
                    _s: PhantomData,
                }
            }

            fn file_name(&self) -> &String<12> {
                &self.file_name
            }

            fn handle_mut(&mut self) -> Option<&mut FATFile> {
                self.handle.as_mut()
            }
        }

        impl<S: FileState> Debug for $s<S> {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str(&self.file_name)
            }
        }

        impl $s<Closed> {
            pub async fn open_read<D: BlockDevice, TS: TimeSource>(
                self,
                fs: &mut FileSystem<D, TS>,
            ) -> Result<$s<OpenRead>, StdlibErrorFileWrapper<$s<Closed>>> {
                let name = self.file_name().clone();
                let f = open_file(
                    &mut fs.controller,
                    &mut fs.volume,
                    self,
                    file_dir!(fs, $s),
                    Mode::ReadOnly,
                )
                .await?;

                Ok($s::<OpenRead>::init_read(Some(f), &name))
            }

            pub async fn open_write<D: BlockDevice, TS: TimeSource>(
                self,
                fs: &mut FileSystem<D, TS>,
                replace: bool,
            ) -> Result<$s<OpenWrite>, StdlibErrorFileWrapper<$s<Closed>>> {
                let name = self.file_name().clone();
                let f = open_file(
                    &mut fs.controller,
                    &mut fs.volume,
                    self,
                    file_dir!(fs, $s),
                    if replace {
                        Mode::ReadWriteCreateOrTruncate
                    } else {
                        Mode::ReadWriteCreateOrAppend
                    },
                )
                .await?;

                Ok($s::<OpenWrite>::init_write(Some(f), &name))
            }
        }

        impl $s<OpenWrite> {
            pub async fn dump<D: BlockDevice, TS: TimeSource, S: Serialize>(
                &mut self,
                fs: &mut FileSystem<D, TS>,
                data: &S,
            ) -> Result<(), StdlibError> {
                let mut buffer = [0u8; FILE_BUFFER_SIZE];
                into_writer(data, &mut buffer[..])?;
                fs.controller
                    .write(&mut fs.volume, self.handle_mut().unwrap(), &buffer)
                    .await?;
                Ok(())
            }

            pub async fn dump_bytes<D: BlockDevice, TS: TimeSource>(
                &mut self,
                fs: &mut FileSystem<D, TS>,
                data: &[u8],
            ) -> Result<(), StdlibError> {
                fs.controller
                    .write(&mut fs.volume, self.handle_mut().unwrap(), data)
                    .await?;
                Ok(())
            }

            pub fn close<D: BlockDevice, TS: TimeSource>(
                &mut self,
                fs: &mut FileSystem<D, TS>,
            ) -> Result<(), StdlibError> {
                fs.controller
                    .close_file(&mut fs.volume, self.handle.take().unwrap())?;
                Ok(())
            }
        }

        impl $s<OpenRead> {
            pub async fn load<'t, D: BlockDevice, TS: TimeSource, DS: DeserializeOwned>(
                &'t mut self,
                fs: &'t mut FileSystem<D, TS>,
            ) -> Result<DS, StdlibError> {
                let mut buffer = [0u8; FILE_BUFFER_SIZE];
                fs.controller
                    .read(&fs.volume, self.handle_mut().unwrap(), &mut buffer)
                    .await?;
                let res: Result<DS, _> = from_reader(&buffer[..]);
                Ok(res?)
            }

            pub fn close<D: BlockDevice, TS: TimeSource>(
                &mut self,
                fs: &mut FileSystem<D, TS>,
            ) -> Result<(), StdlibError> {
                fs.controller
                    .close_file(&mut fs.volume, self.handle.take().unwrap())?;
                Ok(())
            }
        }
    };
}

file_impl!(DataFile);
file_impl!(ConfigFile);
file_impl!(BinFile);
