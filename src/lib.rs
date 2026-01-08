#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc = include_str!("../README.md")]

#[cfg(not(any(windows, unix)))]
compile_error!("bevy-histrion-packer is not supported on this platform");

mod encoding;
mod format;

use std::path::PathBuf;

use bevy::{
    asset::io::{AssetReaderError, AssetSource, AssetSourceBuilder, AssetSourceId},
    prelude::*,
};
use thiserror::Error;

pub use format::{CompressionMethod, HpakReader};

/// The magic number identifying HPAK files (ASCII "HPAK").
///
/// This appears at the start of every HPAK archive and is used to
/// quickly validate that a file is in the correct format.
pub const MAGIC: [u8; 4] = *b"HPAK";

/// The current version of the HPAK file format.
///
/// This version number is stored in the archive header.
pub const VERSION: u32 = 6;

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("the archive has already been finalized")]
    AlreadyFinalized,
    #[error("duplicated hpak entry: {0}")]
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
    #[error("encountered an invalid alignment: {0}, must be a power of 2")]
    InvalidAlignment(u64),
    #[error("encountered an invalid utf8 error: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
}

impl From<Error> for AssetReaderError {
    fn from(err: Error) -> Self {
        use Error::*;

        match err {
            EntryNotFound(path) => AssetReaderError::NotFound(path),
            Io(err) => AssetReaderError::Io(err.into()),
            err => AssetReaderError::Io(std::io::Error::other(format!("{}", err)).into()),
        }
    }
}

/// Configuration mode for how the HistrionPackerPlugin integrates with Bevy's asset system.
#[cfg_attr(feature = "debug-impls", derive(Debug))]
#[derive(Clone, Default)]
pub enum HistrionPackerMode {
    /// Add a new [`AssetSource`] available through the `<source_id>://` source.
    ///
    /// This mode creates an additional asset source that can be accessed with a custom prefix.
    /// For example, with `Autoload("packed")`, assets can be loaded using `packed://path/to/asset`.
    Autoload(&'static str),

    /// Replace the default [`AssetSource`] with the HPAK source for processed files only.
    ///
    /// In this mode, the plugin intercepts only processed asset loads and serves them from
    /// the HPAK archive, while unprocessed assets are still loaded from the filesystem.
    /// This is the recommended mode for production builds.
    ///
    /// **Important**: This plugin must be added **before** `AssetPlugin` in the plugin chain.
    #[default]
    ReplaceDefaultProcessed,
}

/// Bevy plugin for loading assets from HPAK archives.
///
/// This plugin integrates with Bevy's asset system to load assets from a packed HPAK archive
/// instead of (or in addition to) loose files on disk.
///
/// # Examples
///
/// ```no_run
/// use bevy::prelude::*;
/// use bevy_histrion_packer::{HistrionPackerPlugin, HistrionPackerMode};
///
/// App::new()
///     .add_plugins(
///         DefaultPlugins
///             .build()
///             .add_before::<AssetPlugin>(HistrionPackerPlugin {
///                 source: "assets.hpak".to_string(),
///                 mode: HistrionPackerMode::ReplaceDefaultProcessed,
///             })
///             .set(AssetPlugin {
///                 mode: AssetMode::Processed,
///                 ..default()
///             }),
///     )
///     .run();
/// ```
pub struct HistrionPackerPlugin {
    /// Path to the HPAK archive file, relative to the executable location.
    pub source: String,

    /// Integration mode determining how the plugin interacts with Bevy's asset system.
    pub mode: HistrionPackerMode,
}

impl Plugin for HistrionPackerPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        let source = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(err) => {
                error!("cannot get current executable path: {err}");
                return;
            }
        };

        if !source.exists() {
            error!("the source path does not exist or is not a file");
            return;
        }

        let mut source = match source.canonicalize() {
            Ok(path) => path,
            Err(err) => {
                error!("cannot canonicalize current executable path: {err}");
                return;
            }
        };

        source.pop();
        source.push(&self.source);

        match self.mode {
            HistrionPackerMode::Autoload(source_id) => {
                app.register_asset_source(
                    AssetSourceId::Name(source_id.into()),
                    AssetSourceBuilder::new(|| {
                        AssetSource::get_default_reader("assets".to_string())()
                    })
                    .with_processed_reader(move || {
                        let source = source.clone();
                        Box::new(HpakReader::new(&source).unwrap())
                    }),
                );
            }
            HistrionPackerMode::ReplaceDefaultProcessed => {
                if app.is_plugin_added::<AssetPlugin>() {
                    error!("plugin HistrionPackerPlugin must be added before plugin AssetPlugin");
                    return;
                }

                app.register_asset_source(
                    AssetSourceId::Default,
                    AssetSourceBuilder::new(|| {
                        AssetSource::get_default_reader("assets".to_string())()
                    })
                    .with_processed_reader(move || {
                        let source = source.clone();
                        Box::new(HpakReader::new(&source).unwrap())
                    }),
                );
            }
        }
    }
}

/// Writer module for creating HPAK archives.
///
/// This module is only available when the `writer` feature is enabled.
/// It provides the `HpakWriter` type and related utilities for packing
/// assets into HPAK archives.
#[cfg(feature = "writer")]
#[cfg_attr(docsrs, doc(cfg(feature = "writer")))]
pub mod writer {
    use super::*;

    pub use format::writer::*;
}
