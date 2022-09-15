use alloc::{boxed::Box, vec::Vec};
use ciborium::{ser::into_writer, de::from_reader, value::Value};
use core::{marker::PhantomData, str, fmt::Debug};
use embedded_sdmmc::{
    BlockDevice, Controller, Directory, File as FATFile, Mode, ShortFileName, TimeSource, Volume,
    VolumeIdx,
};
use heapless::String;
use serde::{self, Deserialize, Serialize};
use ufmt::{uDisplay, uWrite, uwrite, Formatter};

use crate::log;

use super::{StdlibError, StdlibErrorFileWrapper};

struct FileNameWrapper<'a>(&'a ShortFileName);

pub trait FileState {}

#[derive(Debug)]
pub struct OpenRead;
impl FileState for OpenRead {}

#[derive(Debug)]
pub struct OpenWrite;
impl FileState for OpenWrite {}

#[derive(Debug)]
pub struct Closed;
impl FileState for Closed {}

pub trait FileContent: Debug + Send {
    fn serialize(&self, buf: &mut [u8]) -> Result<(), ciborium::ser::Error<ciborium_io::OutOfSpace>>;
}

impl<T: Serialize + Debug + Send> FileContent for T {
    fn serialize(&self, buf: &mut [u8]) -> Result<(), ciborium::ser::Error<ciborium_io::OutOfSpace>> {
        into_writer(self, buf)?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct File<S: FileState> {
    pub dir: String<8>,
    pub file_name: String<12>,
    #[serde(skip)]
    pub handle: Option<FATFile>,

    _s: PhantomData<S>
}

impl<S: FileState> File<S> {
    pub fn new(dir: &str, file_name: &str) -> Self {
        Self {
            dir: dir.into(),
            file_name: file_name.into(),
            handle: None,
            _s: PhantomData
        }
    }

    pub fn init_read(handle: Option<FATFile>, dir: &str, file_name: &str) -> Self {
        Self {
            dir: dir.into(),
            file_name: file_name.into(),
            handle,
            _s: PhantomData
        }
    }

    pub fn init_write(handle: Option<FATFile>, dir: &str, file_name: &str) -> Self {
        Self {
            dir: dir.into(),
            file_name: file_name.into(),
            handle,
            _s: PhantomData
        }
    }

    fn file_name(&self) -> &String<12> {
        &self.file_name
    }

    fn handle_mut(&mut self) -> Option<&mut FATFile> {
        self.handle.as_mut()
    }
}

impl File<Closed> {
    pub async fn open_read<D: BlockDevice, TS: TimeSource>(
        self,
        fs: &mut FileSystem<D, TS>,
    ) -> Result<File<OpenRead>, StdlibErrorFileWrapper> {
        let name = self.file_name().clone();
        let dir = self.dir.clone();
        let f = open_file(
            &mut fs.controller,
            &mut fs.volume,
            self,
            Mode::ReadOnly,
        )
        .await?;

        Ok(File::<OpenRead>::init_read(Some(f), &dir, &name))
    }

    pub async fn open_write<D: BlockDevice, TS: TimeSource>(
        self,
        fs: &mut FileSystem<D, TS>,
        replace: bool,
    ) -> Result<File<OpenWrite>, StdlibErrorFileWrapper> {
        let name = self.file_name().clone();
        let dir = self.dir.clone();
        let f = open_file(
            &mut fs.controller,
            &mut fs.volume,
            self,
            if replace {
                Mode::ReadWriteCreateOrTruncate
            } else {
                Mode::ReadWriteCreateOrAppend
            },
        )
        .await?;

        Ok(File::<OpenWrite>::init_write(Some(f), &dir, &name))
    }
}

impl File<OpenWrite> {
    pub async fn dump<D: BlockDevice, TS: TimeSource, S: FileContent + ?Sized>(
        &mut self,
        fs: &mut FileSystem<D, TS>,
        data: &S,
    ) -> Result<(), StdlibError> {
        let mut buffer = [0u8; FILE_BUFFER_SIZE];
        data.serialize(&mut buffer[..])?;
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

impl File<OpenRead> {
    pub async fn load<'t, D: BlockDevice, TS: TimeSource>(
        &'t mut self,
        fs: &'t mut FileSystem<D, TS>,
    ) -> Result<Value, StdlibError> {
        let mut buffer = [0u8; FILE_BUFFER_SIZE];
        fs.controller
            .read(&fs.volume, self.handle_mut().unwrap(), &mut buffer)
            .await?;
        Ok(from_reader(&buffer[..])?)
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
}

const FILE_BUFFER_SIZE: usize = 4096; // 4KB

impl<D: BlockDevice, TS: TimeSource> FileSystem<D, TS> {
    pub async fn list_files(
        &mut self,
        dir_name: &str
    ) -> Result<Vec<File<Closed>>, StdlibError> {
        let mut res = Vec::new();
    
        let root = self.controller.open_root_dir(&self.volume)?;
        let dir = self.controller.open_dir(&self.volume, &root, dir_name).await?;
    
        self.controller
            .iterate_dir(&self.volume, &dir, |e| {
                let mut text = String::<12>::new();
                uwrite!(text, "{}", FileNameWrapper(&e.name)).unwrap();
                // this is basically infallible (unless, I f*ed up, which is not that unlikely)
                res.push(File::new(dir_name, &text));
            })
            .await?;
        Ok(res)
    }
}

impl<D: BlockDevice, TS: TimeSource> FileSystem<D, TS> {
    pub async fn new(block_device: D, timesource: TS) -> Result<FileSystem<D, TS>, StdlibError> {
        let mut controller = Controller::new(block_device, timesource);
        let volume = controller
            .get_volume(VolumeIdx(0))
            .await?;
        Ok(Self {
            controller,
            volume
        })
    }

}

async fn open_file<D: BlockDevice, TS: TimeSource>(
    controller: &mut Controller<D, TS>,
    volume: &mut Volume,
    file: File<Closed>,
    mode: Mode,
) -> Result<FATFile, StdlibErrorFileWrapper> {
    let root = controller.open_root_dir(volume).map_err(|e| StdlibErrorFileWrapper(e.into(), None))?;
    let dir = controller.open_dir(volume, &root, &file.dir).await.map_err(|e| StdlibErrorFileWrapper(e.into(), None))?;
    let res = controller
        .open_file_in_dir(volume, &dir, &file.file_name(), mode)
        .await
        .map_err(|e| StdlibErrorFileWrapper(e.into(), Some(file)));
    controller.close_dir(volume, dir);
    controller.close_dir(volume, root);
    res
}
