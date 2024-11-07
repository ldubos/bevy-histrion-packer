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

pub use format::{CompressionMethod, HpakReader};

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

#[cfg(feature = "writer")]
pub mod writer {
    use super::*;
    use bevy::utils::HashMap;
    pub use format::HpakWriter;
    use std::collections::BTreeMap;
    use std::fs;

    use std::path::Path;

    pub fn default_extensions_compression_method() -> Option<HashMap<String, CompressionMethod>> {
        HashMap::from([
            // audio
            ("ogg".to_string(), CompressionMethod::Deflate),
            ("oga".to_string(), CompressionMethod::Deflate),
            ("spx".to_string(), CompressionMethod::Deflate),
            ("mp3".to_string(), CompressionMethod::Deflate),
            ("qoa".to_string(), CompressionMethod::Deflate),
            // image
            ("exr".to_string(), CompressionMethod::None),
            ("png".to_string(), CompressionMethod::None),
            ("jpg".to_string(), CompressionMethod::None),
            ("jpeg".to_string(), CompressionMethod::None),
            ("webp".to_string(), CompressionMethod::Deflate),
            ("ktx".to_string(), CompressionMethod::None),
            ("ktx2".to_string(), CompressionMethod::None),
            ("basis".to_string(), CompressionMethod::None),
            ("qoi".to_string(), CompressionMethod::Deflate),
            ("dds".to_string(), CompressionMethod::None),
            ("tga".to_string(), CompressionMethod::Deflate),
            ("bmp".to_string(), CompressionMethod::None),
            // 3d models
            ("gltf".to_string(), CompressionMethod::Deflate),
            ("glb".to_string(), CompressionMethod::Deflate),
            ("obj".to_string(), CompressionMethod::Deflate),
            ("fbx".to_string(), CompressionMethod::Deflate),
            ("meshlet_mesh".to_string(), CompressionMethod::Deflate),
            // shaders
            ("glsl".to_string(), CompressionMethod::Deflate),
            ("hlsl".to_string(), CompressionMethod::Deflate),
            ("vert".to_string(), CompressionMethod::Deflate),
            ("frag".to_string(), CompressionMethod::Deflate),
            ("vs".to_string(), CompressionMethod::Deflate),
            ("fs".to_string(), CompressionMethod::Deflate),
            ("wgsl".to_string(), CompressionMethod::Deflate),
            ("spv".to_string(), CompressionMethod::Deflate),
            ("metal".to_string(), CompressionMethod::Deflate),
            // text
            ("txt".to_string(), CompressionMethod::Deflate),
            ("toml".to_string(), CompressionMethod::Deflate),
            ("ron".to_string(), CompressionMethod::Deflate),
            ("json".to_string(), CompressionMethod::Deflate),
            ("yaml".to_string(), CompressionMethod::Deflate),
            ("yml".to_string(), CompressionMethod::Deflate),
            ("xml".to_string(), CompressionMethod::Deflate),
            ("md".to_string(), CompressionMethod::Deflate),
            // video
            ("mp4".to_string(), CompressionMethod::Deflate),
            ("webm".to_string(), CompressionMethod::Deflate),
        ])
        .into()
    }

    /// Pack all assets presents either in the `assets_dir` or in the `processed_dir` directory,
    /// into a single `output_file` HPAK file.
    ///
    /// It will first look for all assets in the `processed_dir` directory, and then in the
    /// `assets_dir` directory.
    ///
    /// The `meta_compression_method` is used to compress the metadata of the assets.
    /// The `default_compression_method` is used to compress the data of the assets if no `method`
    /// is specified in the `extensions_compression_method` map for the asset's extension.
    ///
    /// If `with_padding` is true, padding will be added to align entries to 4096 bytes.
    #[allow(clippy::too_many_arguments)]
    pub fn pack_assets_folder(
        assets_dir: impl AsRef<Path>,
        processed_dir: impl AsRef<Path>,
        output_file: impl AsRef<Path>,
        meta_compression_method: CompressionMethod,
        default_compression_method: CompressionMethod,
        extensions_compression_method: Option<HashMap<String, CompressionMethod>>,
        ignore_missing_meta: bool,
        with_padding: bool,
    ) -> Result<()> {
        let mut writer = HpakWriter::new(output_file, meta_compression_method, with_padding)?;
        let mut assets_map: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();

        for source in [processed_dir.as_ref(), assets_dir.as_ref()] {
            if !source.exists() {
                return Err(Error::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("source directory does not exist: {source:?}"),
                )));
            }

            if !source.is_dir() {
                return Err(Error::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("source is not a directory: {source:?}"),
                )));
            }

            for entry in walkdir(source) {
                let extension = entry.extension().unwrap_or_default().to_os_string();

                if extension.eq("meta") {
                    continue;
                }

                let key = match entry.strip_prefix(source) {
                    Ok(path) => path.to_path_buf(),
                    Err(e) => {
                        return Err(Error::Io(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("invalid path: {e}"),
                        )))
                    }
                };

                if assets_map.contains_key(&key) {
                    continue;
                }

                assets_map.insert(key.clone(), entry.clone());
            }
        }

        let assets_map = assets_map.into_iter().collect::<Vec<_>>();
        let extensions_compression_method = extensions_compression_method.as_ref();

        for (entry, path) in assets_map {
            let meta_path = get_meta_path(&path);

            let mut meta_file: Box<dyn std::io::Read> = if meta_path.exists() {
                Box::new(fs::File::open(&meta_path)?)
            } else if ignore_missing_meta {
                Box::new(std::io::Cursor::new(vec![]))
            } else {
                continue;
            };

            let mut data_file = fs::File::open(&path)?;

            let extension = path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let compression_method = extensions_compression_method
                .and_then(|extensions| extensions.get(&extension).copied())
                .unwrap_or(default_compression_method);

            writer.add_entry(&entry, &mut meta_file, &mut data_file, compression_method)?;
        }

        writer.finalize()?;

        Ok(())
    }

    fn get_meta_path(path: impl AsRef<Path>) -> PathBuf {
        let mut meta_path = path.as_ref().to_path_buf();
        let mut extension = meta_path.extension().unwrap_or_default().to_os_string();
        extension.push(".meta");
        meta_path.set_extension(extension);
        meta_path
    }

    fn walkdir<'a>(root: impl AsRef<Path>) -> Box<dyn Iterator<Item = PathBuf> + 'a> {
        Box::new(
            fs::read_dir(root.as_ref())
                .unwrap()
                .filter_map(|entry| match entry {
                    Ok(entry) => {
                        let path = entry.path();

                        if path.is_dir() {
                            Some(walkdir(path).collect::<Vec<_>>())
                        } else {
                            Some(vec![path])
                        }
                    }
                    Err(_) => None,
                })
                .flatten(),
        )
    }
}
