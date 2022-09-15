use alloc::{format, string::String};
use ciborium::{de::Error as CBORDeserializerError, ser::Error as CBORSerializerError};
use core::fmt::{Debug, Display};
use embedded_sdmmc::{Error as ESDMMCError};

use super::{Closed, File};

pub struct StdlibErrorFileWrapper(pub StdlibError, pub Option<File<Closed>>);

impl Debug for StdlibErrorFileWrapper {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("StdlibErrorFileWrapper")
            .field(&self.0)
            .field(&self.1)
            .finish()
    }
}

#[derive(Debug)]
pub enum StdlibError {
    FS(FSError),
    Serialization,
    Deserialization,
    TaskInterface(String)
}

#[derive(Debug)]
pub enum FSError {
    DeviceError(String),
    FormatError(String),
    NoSuchVolume,
    FilenameError(String),
    TooManyOpenDirs,
    TooManyOpenFiles,
    FileNotFound,
    FileAlreadyOpen,
    DirAlreadyOpen,
    OpenedDirAsFile,
    DeleteDirAsFile,
    FileIsOpen,
    Unsupported,
    EndOfFile,
    BadCluster,
    ConversionError,
    NotEnoughSpace,
    AllocationError,
    JumpedFree,
    ReadOnly,
    FileAlreadyExists,
    BadBlockSize(u16),
    NotInBlock,
}

impl<E: Debug> From<ESDMMCError<E>> for StdlibError {
    fn from(err: ESDMMCError<E>) -> Self {
        StdlibError::FS(err.into())
    }
}

impl<E: Debug> From<ESDMMCError<E>> for FSError {
    fn from(err: ESDMMCError<E>) -> Self {
        match err {
            ESDMMCError::DeviceError(e) => FSError::DeviceError(format!("{:?}", e)),
            ESDMMCError::FormatError(e) => FSError::FormatError(e.into()),
            ESDMMCError::NoSuchVolume => FSError::NoSuchVolume,
            ESDMMCError::FilenameError(e) => FSError::FilenameError(format!("{:?}", e)),
            ESDMMCError::TooManyOpenDirs => FSError::TooManyOpenDirs,
            ESDMMCError::TooManyOpenFiles => FSError::TooManyOpenFiles,
            ESDMMCError::FileNotFound => FSError::FileNotFound,
            ESDMMCError::FileAlreadyOpen => FSError::FileAlreadyOpen,
            ESDMMCError::DirAlreadyOpen => FSError::DirAlreadyOpen,
            ESDMMCError::OpenedDirAsFile => FSError::OpenedDirAsFile,
            ESDMMCError::DeleteDirAsFile => FSError::DeleteDirAsFile,
            ESDMMCError::FileIsOpen => FSError::FileIsOpen,
            ESDMMCError::Unsupported => FSError::Unsupported,
            ESDMMCError::EndOfFile => FSError::EndOfFile,
            ESDMMCError::BadCluster => FSError::BadCluster,
            ESDMMCError::ConversionError => FSError::ConversionError,
            ESDMMCError::NotEnoughSpace => FSError::NotEnoughSpace,
            ESDMMCError::AllocationError => FSError::AllocationError,
            ESDMMCError::JumpedFree => FSError::JumpedFree,
            ESDMMCError::ReadOnly => FSError::ReadOnly,
            ESDMMCError::FileAlreadyExists => FSError::FileAlreadyExists,
            ESDMMCError::BadBlockSize(s) => FSError::BadBlockSize(s),
            ESDMMCError::NotInBlock => FSError::NotInBlock,
        }
    }
}

impl<T> From<CBORDeserializerError<T>> for StdlibError {
    fn from(_err: CBORDeserializerError<T>) -> Self {
        StdlibError::Deserialization
    }
}

impl<T> From<CBORSerializerError<T>> for StdlibError {
    fn from(_err: CBORSerializerError<T>) -> Self {
        StdlibError::Serialization
    }
}

impl Display for FSError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let txt: String;
        let txt = match self {
            // TODO: decent error serializing here (would require implementing `uDebug` in `embedded_sdmmc::Error`)
            FSError::DeviceError(e) => {
                txt = format!("Device error: {:?}", e);
                txt.as_str()
            },
            FSError::FormatError(e) => {
                txt = format!("Filesystem badly formatted: {}", e);
                &txt
            }
            FSError::NoSuchVolume => "Invalid VolumeIdx",
            FSError::FilenameError(e) => {
                txt = format!("Given file name is invalid: {:?}", e);
                &txt
            },
            FSError::TooManyOpenDirs => "Too many open dirs",
            FSError::TooManyOpenFiles => "Too many open files",
            FSError::FileNotFound => "File not found",
            FSError::FileAlreadyOpen => "File already open",
            FSError::DirAlreadyOpen => "Directory already open",
            FSError::OpenedDirAsFile => "Opening directory as file",
            FSError::DeleteDirAsFile => "Deleting directory as file",
            FSError::FileIsOpen => "File is open",
            FSError::Unsupported => "Unsupported",
            FSError::EndOfFile => "End of file",
            FSError::BadCluster => "Bad cluster",
            FSError::ConversionError => "Conversion error",
            FSError::NotEnoughSpace => "Not enough space in device",
            FSError::AllocationError => "Cluster was not properly allocated by the library",
            FSError::JumpedFree => "Jumped to free space during fat traversing",
            FSError::ReadOnly => "Read Only",
            FSError::FileAlreadyExists => "File already exists",
            FSError::BadBlockSize(n) => {
                txt = format!("Bad block size: {}. Only 512 bytes allowed.", n);
                &txt
            }
            FSError::NotInBlock => "Entry not found in the block",
        };
        f.write_str(txt)
    }
}
