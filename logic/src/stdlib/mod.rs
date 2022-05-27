mod errors;
mod files;

pub use errors::{StdlibError, StdlibErrorFileWrapper};
pub use files::{
    BinFile, Closed, ConfigFile, DataFile, File, FileState, FileSystem, OpenRead, OpenWrite,
};
