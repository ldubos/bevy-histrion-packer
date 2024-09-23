#![cfg_attr(docsrs, feature(doc_auto_cfg))]

#[cfg(not(any(windows, unix)))]
compile_error!("bevy-histrion-packer is not supported on this platform");

mod encoding;
pub mod errors;
mod hpak;
pub mod utils;

/// The fomrat version of the HPAK file format.
pub const VERSION: u16 = 4;

/// The length of the HPAK magic.
pub const MAGIC_LEN: usize = 4;

/// The magic of the HPAK file format.
pub const MAGIC: &[u8; MAGIC_LEN] = b"HPAK";

use bevy::{
    asset::io::{AssetSource, AssetSourceId},
    prelude::*,
};
pub(crate) use encoding::*;
pub use hpak::compression::CompressionAlgorithm;
pub use hpak::reader::HPakAssetsReader;

#[cfg(feature = "writer")]
pub use hpak::writer::{Writer, WriterBuilder};

#[cfg(feature = "writer")]
pub use utils::pack_assets_folder;

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
        Self::Autoload("hpak")
    }
}

pub struct HistrionPackerPlugin {
    pub source: String,
    pub mode: HistrionPackerMode,
}

impl Plugin for HistrionPackerPlugin {
    fn build(&self, app: &mut App) {
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
                        Box::new(HPakAssetsReader::new(&source).unwrap())
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
                            Box::new(HPakAssetsReader::new(&source).unwrap())
                        }),
                );
            }
        }
    }
}
