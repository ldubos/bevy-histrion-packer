#![cfg_attr(docsrs, feature(doc_auto_cfg))]

#[cfg(not(any(windows, unix)))]
compile_error!("bevy-histrion-packer is not supported on this platform");

mod encoding;
mod format;

use std::path::PathBuf;

use bevy::{
    asset::io::{AssetReaderError, AssetSource, AssetSourceId},
    prelude::*,
};
use thiserror::Error;

pub use format::{HpakReader, HpakWriter};

/// The magic of the HPAK file format.
pub const MAGIC: [u8; 4] = *b"HPAK";

/// The fomrat version of the HPAK file format.
pub const VERSION: u32 = 6;

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("cannot add hpak entry after finalize")]
    CannotAddEntryAfterFinalize,
    #[error("duplicate hpak entry: {0}")]
    DuplicateEntry(PathBuf),
    #[error("hpak entry not found: {0}")]
    EntryNotFound(PathBuf),
    #[error("invalid hpak file format")]
    InvalidFileFormat,
    #[error("bad hpak version: {0}")]
    BadVersion(u32),
    #[error("invalid asset metadata: {0}")]
    InvalidAssetMeta(String),
    #[error("encountered an io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("encountered an invalid utf8 error: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
}

impl From<Error> for AssetReaderError {
    fn from(err: Error) -> Self {
        use Error::*;

        match err {
            EntryNotFound(path) => AssetReaderError::NotFound(path),
            Io(err) => AssetReaderError::Io(err.into()),
            err => AssetReaderError::Io(
                std::io::Error::new(std::io::ErrorKind::Other, format!("{}", err)).into(),
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub enum HistrionPackerMode {
    /// Add a new [`AssetSource`] available through the `<source_id>://` source.
    Autoload(&'static str),
    /// Replace the default [`AssetSource`] with the hpak source for processed files only,
    ///
    /// it uses the default source for the current platform for unprocessed files.
    ReplaceDefaultProcessed,
}

impl Default for HistrionPackerMode {
    fn default() -> Self {
        Self::ReplaceDefaultProcessed
    }
}

pub struct HistrionPackerPlugin {
    pub source: String,
    pub mode: HistrionPackerMode,
}

impl Plugin for HistrionPackerPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        let source = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(err) => {
                bevy::log::error!("cannot get current executable path: {err}");
                return;
            }
        };

        if !source.exists() {
            bevy::log::error!("the source path does not exist or is not a file");
            return;
        }

        let mut source = match source.canonicalize() {
            Ok(path) => path,
            Err(err) => {
                bevy::log::error!("cannot canonicalize current executable path: {err}");
                return;
            }
        };

        source.pop();
        source.push(&self.source);

        match self.mode {
            HistrionPackerMode::Autoload(source_id) => {
                app.register_asset_source(
                    AssetSourceId::Name(source_id.into()),
                    AssetSource::build().with_reader(move || {
                        let source = source.clone();
                        Box::new(HpakReader::new(&source).unwrap())
                    }),
                );
            }
            HistrionPackerMode::ReplaceDefaultProcessed => {
                if app.is_plugin_added::<AssetPlugin>() {
                    bevy::log::error!(
                        "plugin HistrionPackerPlugin must be added before plugin AssetPlugin"
                    );
                    return;
                }

                app.register_asset_source(
                    AssetSourceId::Default,
                    AssetSource::build()
                        .with_reader(|| AssetSource::get_default_reader("assets".to_string())())
                        .with_processed_reader(move || {
                            let source = source.clone();
                            Box::new(HpakReader::new(&source).unwrap())
                        }),
                );
            }
        }
    }
}
