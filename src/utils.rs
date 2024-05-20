use bevy::asset::{
    meta::{AssetAction, AssetMeta},
    processor::Process,
    AssetLoader,
};
use serde::{Deserialize, Serialize};

use crate::errors::Error;

pub fn get_meta_loader_type_path(meta: &[u8]) -> Result<String, Error> {
    let meta = AssetMeta::<DummyLoader, DummyProcessor<DummyLoader>>::deserialize(meta)
        .map_err(|e| Error::InvalidAssetMeta(e.to_string()))?;

    let loader_type_path = if let AssetAction::Load { loader, .. } = meta.asset {
        loader
    } else {
        return Err(Error::InvalidAssetMeta("Invalid asset action".to_string()));
    };

    Ok(loader_type_path)
}

pub fn get_meta_loader_settings<L: AssetLoader>(meta: &[u8]) -> Result<L::Settings, Error> {
    let meta = AssetMeta::<L, DummyProcessor<L>>::deserialize(meta)
        .map_err(|e| Error::InvalidAssetMeta(e.to_string()))?;

    let settings = if let AssetAction::Load { settings, .. } = meta.asset {
        settings
    } else {
        return Err(Error::InvalidAssetMeta("Invalid asset action".to_string()));
    };

    Ok(settings)
}

#[cfg(feature = "packing")]
pub use packing::*;

#[cfg(feature = "packing")]
mod packing {
    use std::{
        fs::File,
        path::{Path, PathBuf},
    };

    use crate::Writer;

    fn get_meta_path(path: &Path) -> PathBuf {
        let mut meta_path = path.to_path_buf();
        let mut extension = path
            .extension()
            .expect("asset paths must have extensions")
            .to_os_string();
        extension.push(".meta");
        meta_path.set_extension(extension);
        meta_path
    }

    /// Read the `source` folder recursively and pack all it's assets into a HPAK file.
    ///
    /// # Examples
    ///
    /// ## Basic usage
    ///
    /// ```
    /// use std::fs::{File, OpenOptions};
    /// use std::path::Path;
    /// use bevy_histrion_packer::{WriterBuilder, CompressionAlgorithm, pack_assets_folder};
    ///
    /// let source = Path::new("imported_assets/Default");
    /// let mut destination = OpenOptions::new()
    ///         .write(true)
    ///         .create(true)
    ///         .truncate(true)
    ///         .open("assets.hpak").unwrap();
    ///
    /// let mut writer = WriterBuilder::new(&mut destination).build().unwrap();
    ///
    /// pack_assets_folder(&source, &mut writer).unwrap();
    /// ```
    ///
    /// ## Custom config
    ///
    /// ```
    /// use std::fs::{File, OpenOptions};
    /// use std::path::Path;
    /// use bevy_histrion_packer::{WriterBuilder, CompressionAlgorithm, pack_assets_folder};
    ///
    /// let source = Path::new("imported_assets/Default");
    /// let mut destination = OpenOptions::new()
    ///         .write(true)
    ///         .create(true)
    ///         .truncate(true)
    ///         .open("assets.hpak").unwrap();
    ///
    /// // Use Deflate compression for metadata and data.
    /// let mut writer = WriterBuilder::new(&mut destination)
    ///         .meta_compression(CompressionAlgorithm::Deflate)
    ///         .data_compression_fn(&|_path, _meta| CompressionAlgorithm::Deflate)
    ///         .build().unwrap();
    ///
    /// pack_assets_folder(&source, &mut writer).unwrap();
    /// ```
    pub fn pack_assets_folder<W: std::io::Write>(
        source: &Path,
        writer: &mut Writer<W>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for entry in walkdir::WalkDir::new(source)
            .into_iter()
            .filter_map(Result::ok)
        {
            let data_path = entry.path();
            let extension = data_path.extension().unwrap_or_default().to_os_string();

            if data_path.is_file() && !extension.eq("meta") {
                let meta_path = get_meta_path(data_path);

                if !meta_path.exists() {
                    continue;
                }

                let mut meta_file = File::open(meta_path)?;
                let mut data_file = File::open(data_path)?;

                writer.add_entry(
                    data_path.strip_prefix(source)?,
                    &mut meta_file,
                    &mut data_file,
                )?;
            }
        }

        writer.finish()?;
        Ok(())
    }
}

// hack to deserialize any AssetLoader
#[derive(Default, Deserialize, Serialize)]
struct DummySettings {
    #[serde(default)]
    _dummy: bool,
}

struct DummyLoader;

impl AssetLoader for DummyLoader {
    type Asset = ();

    type Settings = DummySettings;

    type Error = std::io::Error;

    fn load<'a>(
        &'a self,
        _reader: &'a mut bevy::asset::io::Reader,
        _settings: &'a Self::Settings,
        _load_context: &'a mut bevy::asset::LoadContext,
    ) -> bevy::utils::BoxedFuture<'a, Result<Self::Asset, Self::Error>> {
        Box::pin(async move { Ok(()) })
    }
}
struct DummyProcessor<L: AssetLoader>(std::marker::PhantomData<L>);

impl<L: AssetLoader> Process for DummyProcessor<L> {
    type Settings = ();

    type OutputLoader = L;

    fn process<'a>(
        &'a self,
        _context: &'a mut bevy::asset::processor::ProcessContext,
        _meta: AssetMeta<(), Self>,
        _writer: &'a mut bevy::asset::io::Writer,
    ) -> bevy::utils::BoxedFuture<
        'a,
        Result<<Self::OutputLoader as AssetLoader>::Settings, bevy::asset::processor::ProcessError>,
    > {
        Box::pin(async move { Ok(<Self::OutputLoader as AssetLoader>::Settings::default()) })
    }
}
