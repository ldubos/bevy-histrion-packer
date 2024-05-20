use std::path::Path;

use bevy::{asset::io::AssetReaderError, utils::thiserror};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("entry not found")]
    NotFound,
    #[error("cannot add entry after finalize call")]
    CannotAddEntryAfterFinalize,
    #[error("invalid file format")]
    InvalidFileFormat,
    #[error("bad version: ${0}")]
    BadVersion(u16),
    #[error("invalid asset metadata: {0}")]
    InvalidAssetMeta(String),
    #[error("encountered an io error: {0}")]
    Io(#[from] std::io::Error),
}

impl Error {
    /// Converts this error to an [`AssetReaderError`].
    pub(crate) fn into_asset_reader_error(self, path: &Path) -> AssetReaderError {
        match self {
            Error::NotFound => AssetReaderError::NotFound(path.to_path_buf()),
            Error::Io(err) => AssetReaderError::Io(err.into()),
            _ => unreachable!(),
        }
    }
}
