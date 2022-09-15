use crate::{stdlib::{
    Closed, File,
}};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub(super) struct Config {
    current_data_file: Option<File<Closed>>,
}

impl Default for Config {
    fn default() -> Self {
        Self { current_data_file: None }
    }
}