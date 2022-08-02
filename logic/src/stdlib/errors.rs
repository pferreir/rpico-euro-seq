use alloc::{format, borrow::ToOwned, string::String};
use embedded_sdmmc::{Error as ESDMMCError, BlockDevice, sdmmc::Error as ESCMMCSPIError};
use ciborium::{de::Error as CBORDeserializerError, ser::Error as CBORSerializerError};
use ciborium_io::{OutOfSpace, EndOfFile};
use core::{
    fmt::Debug
};

use super::{File, Closed};

pub struct StdlibErrorFileWrapper<D: BlockDevice, F: File<Closed>>(pub StdlibError<D>, pub F);

impl<D: BlockDevice, F: File<Closed>> Debug for StdlibErrorFileWrapper<D, F> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("StdlibErrorFileWrapper").field(&self.0).field(&self.1).finish()
    }
}

pub enum StdlibError<D: BlockDevice> {
    Device(ESDMMCError<<D as BlockDevice>::Error>),
    SPI(ESCMMCSPIError),
    Serialization(CBORSerializerError<OutOfSpace>),
    Deserialization(CBORDeserializerError<EndOfFile>),
}

impl<D: BlockDevice> Debug for StdlibError<D> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&match self {
            StdlibError::Device(e) => {
                format_sdmmc_error::<D>(e)
            },
            StdlibError::Serialization(_) => "Serialization error".to_owned(),
            StdlibError::Deserialization(_) => "Deseralization error".to_owned(),
            StdlibError::SPI(e) => format!("SPI error: {:?}", e),
        })?;

        Ok(())
    }
}

fn format_sdmmc_error<'t, D: BlockDevice>(e: &ESDMMCError<<D as BlockDevice>::Error>) -> String {
    match e {
        // TODO: decent error serializing here (would require implementing `uDebug` in `embedded_sdmmc::Error`)
        ESDMMCError::DeviceError(_) => "Device error".to_owned(),
        ESDMMCError::FormatError(e) => {
            format!("Filesystem badly formatted: {}", e)
        },
        ESDMMCError::NoSuchVolume => "Invalid VolumeIdx".to_owned(),
        ESDMMCError::FilenameError(_) => "Given file name is invalid".to_owned(),
        ESDMMCError::TooManyOpenDirs => "Too many open dirs".to_owned(),
        ESDMMCError::TooManyOpenFiles => "Too many open files".to_owned(),
        ESDMMCError::FileNotFound => "File not found".to_owned(),
        ESDMMCError::FileAlreadyOpen => "File already open".to_owned(),
        ESDMMCError::DirAlreadyOpen => "Directory already open".to_owned(),
        ESDMMCError::OpenedDirAsFile => "Opening directory as file".to_owned(),
        ESDMMCError::DeleteDirAsFile => "Deleting directory as file".to_owned(),
        ESDMMCError::FileIsOpen => "File is open".to_owned(),
        ESDMMCError::Unsupported => "Unsupported".to_owned(),
        ESDMMCError::EndOfFile => "End of file".to_owned(),
        ESDMMCError::BadCluster => "Bad cluster".to_owned(),
        ESDMMCError::ConversionError => "Conversion error".to_owned(),
        ESDMMCError::NotEnoughSpace => "Not enough space in device".to_owned(),
        ESDMMCError::AllocationError => "Cluster was not properly allocated by the library".to_owned(),
        ESDMMCError::JumpedFree => "Jumped to free space during fat traversing".to_owned(),
        ESDMMCError::ReadOnly => "Read Only".to_owned(),
        ESDMMCError::FileAlreadyExists => "File already exists".to_owned(),
        ESDMMCError::BadBlockSize(n) => {
            format!("Bad block size: {}. Only 512 bytes allowed.", n)
        },
        ESDMMCError::NotInBlock => "Entry not found in the block".to_owned(),
        _ => "Unknown error".to_owned()
    }
}
