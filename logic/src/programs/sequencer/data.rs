use heapless::String;
use serde::{Serialize, Deserialize};
use ufmt::uwrite;

use crate::{util::DiscreetUnwrap, stdlib::Closed};
use crate::stdlib::File;

const FILE_BUFFER_SIZE: usize = 10240;

#[derive(Serialize, Deserialize, Debug)]
pub(super) struct SequenceFile {
    seq_name: String<8>
}

impl SequenceFile {

    pub(crate) fn new(seq_name: &str) -> Self {
        Self { seq_name: seq_name.into() }
    }

    fn _load_data_file(&self) -> File<Closed> {
        let mut tmp = String::<12>::new();
        uwrite!(tmp, "{}.seq", &self.seq_name as &str).duwrp();
        File::new("data", &tmp)
    }

    pub(crate) fn set_name(&mut self, file_name: &str) {
        self.seq_name = file_name.into();
    }
}
