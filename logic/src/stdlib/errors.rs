use embedded_sdmmc::{Error as ESDMMCError, BlockDevice};
use heapless::String;
use rmp_serde::{decode::Error as RMPDeserializerError, encode::Error as RMPSerializerError};
use ufmt::{uDisplay, Formatter, uWrite, uwrite};
use core::{
    fmt::Debug
};

use super::{File, Closed};

pub struct StdlibErrorFileWrapper<D: BlockDevice, F: File<Closed>>(pub StdlibError<D>, pub F);

pub enum StdlibError<D: BlockDevice> {
    Device(ESDMMCError<<D as BlockDevice>::Error>),
    Serialization(RMPSerializerError),
    Deserialization(RMPDeserializerError),
}

impl<D: BlockDevice> uDisplay for StdlibError<D> {
    fn fmt<W>(&self, formatter: &mut Formatter<W>) -> Result<(), W::Error>
    where
        W: uWrite + ?Sized,
    {
        let text: String<256>;

        formatter.write_str(match self {
            StdlibError::Device(e) => {
                text = format_sdmmc_error::<D>(e);
                &text
            },
            StdlibError::Serialization(_) => "Serialization error",
            StdlibError::Deserialization(_) => "Deseralization error",
        })?;

        Ok(())
    }
}

impl<D: BlockDevice> Debug for StdlibError<D> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut text: String<256> = String::new();
        uwrite!(text, "{}", self).unwrap();
        f.write_str(&text)?;
        Ok(())
    }
}

fn format_sdmmc_error<'t, D: BlockDevice>(e: &ESDMMCError<<D as BlockDevice>::Error>) -> String<256> {
    let mut text: String<256> = String::new();

    String::from(match e {
        // TODO: decent error serializing here (would require implementing `uDebug` in `embedded_sdmmc::Error`)
        ESDMMCError::DeviceError(_) => "Device error".into(),
        ESDMMCError::FormatError(e) => {
            uwrite!(text, "Filesystem badly formatted: {}", e).unwrap();
            text
        },
        ESDMMCError::NoSuchVolume => "Invalid VolumeIdx".into(),
        ESDMMCError::FilenameError(_) => "Given file name is invalid".into(),
        ESDMMCError::TooManyOpenDirs => "Too many open dirs".into(),
        ESDMMCError::TooManyOpenFiles => "Too many open files".into(),
        ESDMMCError::FileNotFound => "File not found".into(),
        ESDMMCError::FileAlreadyOpen => "File already open".into(),
        ESDMMCError::DirAlreadyOpen => "Directory already open".into(),
        ESDMMCError::OpenedDirAsFile => "Opening directory as file".into(),
        ESDMMCError::DeleteDirAsFile => "Deleting directory as file".into(),
        ESDMMCError::FileIsOpen => "File is open".into(),
        ESDMMCError::Unsupported => "Unsupported".into(),
        ESDMMCError::EndOfFile => "End of file".into(),
        ESDMMCError::BadCluster => "Bad cluster".into(),
        ESDMMCError::ConversionError => "Conversion error".into(),
        ESDMMCError::NotEnoughSpace => "Not enough space in device".into(),
        ESDMMCError::AllocationError => "Cluster was not properly allocated by the library".into(),
        ESDMMCError::JumpedFree => "Jumped to free space during fat traversing".into(),
        ESDMMCError::ReadOnly => "Read Only".into(),
        ESDMMCError::FileAlreadyExists => "File already exists".into(),
        ESDMMCError::BadBlockSize(n) => {
            uwrite!(text, "Bad block size: {}. Only 512 bytes allowed.", n).unwrap();
            text
        },
        ESDMMCError::NotInBlock => "Entry not found in the block".into(),
        _ => "Unknown error".into()
    })
}
