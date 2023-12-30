use bevy::utils::thiserror;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum HPakError {
    #[error("entry not found")]
    NotFound,
    #[error("encountered an io error: {0}")]
    Io(#[from] std::io::Error),
}
