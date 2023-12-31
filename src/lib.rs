#![doc = include_str!("../README.md")]

use bevy::{
    asset::io::{AssetSource, AssetSourceId},
    prelude::*,
};
use std::{
    fs::{File, OpenOptions},
    io::BufReader,
    path::{Path, PathBuf},
};

use hpak::HPakWriter;
use walkdir::WalkDir;

mod assets_reader;
mod errors;
mod hpak;

pub(crate) use errors::HPakError;

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

/// Packs the source folder into a hpak file.
///
/// `accepts_missing_meta` determines whether the packer should add files with missing meta files.
/// ```rust
/// use std::path::Path;
/// use bevy_histrion_packer::pack_assets_folder;
///
/// let source = Path::new("imported_assets/Default");
/// let destination = Path::new("assets.hpak");
///
/// pack_assets_folder(source, destination, false).unwrap();
/// ```
pub fn pack_assets_folder(
    source: &Path,
    destination: &Path,
    allow_missing_meta: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = HPakWriter::new(
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(destination)?,
    );

    for entry in WalkDir::new(source).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let mut extension = path.extension().unwrap_or_default().to_os_string();

        if path.is_file() && !extension.eq("meta") {
            extension.push(".meta");

            let meta_path = get_meta_path(path);

            let mut meta_file: Box<dyn std::io::Read> = if !meta_path.exists() {
                if !allow_missing_meta {
                    continue;
                }

                let dummy = BufReader::new(&b""[..]);
                Box::new(dummy)
            } else {
                Box::new(File::open(meta_path)?)
            };

            let mut data_file = File::open(path)?;

            writer.add_entry(
                path.to_path_buf().strip_prefix(source)?,
                &mut meta_file,
                &mut data_file,
            )?;
        }
    }

    writer.finalize()?;

    Ok(())
}

/// Bevy plugin to add a new [`AssetSource`] which reads from a hpak file.
pub struct HistrionPackerPlugin {
    pub source: PathBuf,
    pub mode: HistrionPackerMode,
}

#[derive(Debug, Clone, Default)]
pub enum HistrionPackerMode {
    /// Add a new [`AssetSource`] available through the `hpak://` source.
    #[default]
    Autoload,
    /// Replace the default [`AssetSource`] with the hpak source for unprocessed files only.
    ReplaceDefaultUnprocessed,
    /// Replace the default [`AssetSource`] with the hpak source for processed files only,
    ///
    /// it uses the default source for the current platform for unprocessed files.
    ReplaceDefaultProcessed,
}

impl Plugin for HistrionPackerPlugin {
    fn build(&self, app: &mut App) {
        if !self.source.exists() || !self.source.is_file() {
            bevy::log::error!("the source path does not exist or is not a file");
            return;
        }

        let source = self.source.clone();

        match self.mode {
            HistrionPackerMode::Autoload => {
                app.register_asset_source(
                    AssetSourceId::Name("hpak".into()),
                    AssetSource::build().with_reader(move || {
                        let source = source.clone();
                        Box::new(assets_reader::HistrionPakAssetsReader::new(&source).unwrap())
                    }),
                );
            }
            HistrionPackerMode::ReplaceDefaultUnprocessed => {
                if app.is_plugin_added::<AssetPlugin>() {
                    bevy::log::error!(
                        "plugin HistrionPackerPlugin must be added before plugin AssetPlugin"
                    );
                    return;
                }

                app.register_asset_source(
                    AssetSourceId::Default,
                    AssetSource::build().with_reader(move || {
                        let source = source.clone();
                        Box::new(assets_reader::HistrionPakAssetsReader::new(&source).unwrap())
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
                            Box::new(assets_reader::HistrionPakAssetsReader::new(&source).unwrap())
                        }),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{io::Read, path::Path};

    use bevy::asset::AsyncReadExt;
    use futures_lite::{future, StreamExt};

    use crate::hpak::HPakReader;

    use super::*;

    async fn cmp_reader_file(path: &Path, reader: &HPakReader) {
        let mut data = reader.read_data(path).unwrap();
        let mut buf = Vec::new();
        data.read_to_end(&mut buf).await.unwrap();
        let mut original = Vec::new();
        File::open(Path::new("assets/").join(path))
            .unwrap()
            .read_to_end(&mut original)
            .unwrap();
        assert_eq!(buf, original);
    }

    #[test]
    fn test_pack_assets_folder() {
        let source = Path::new("assets");
        let destination = Path::new("assets.hpak");

        pack_assets_folder(source, destination, true).unwrap();

        let reader = HPakReader::new(destination).unwrap();

        future::block_on(async {
            let mut stream = reader.read_directory(Path::new("")).unwrap();
            let mut entries = Vec::new();

            while let Some(entry) = stream.next().await {
                entries.push(entry);
            }

            assert_eq!(entries.len(), 4);
            assert!(entries.iter().any(|p| *p == PathBuf::from("subdir/")));
            assert!(entries.iter().any(|p| *p == PathBuf::from("empty.test")));
            assert!(entries.iter().any(|p| *p == PathBuf::from("test.test")));
            assert!(entries.iter().any(|p| *p == PathBuf::from("テスト.test")));

            cmp_reader_file(Path::new("empty.test"), &reader).await;
            cmp_reader_file(Path::new("test.test"), &reader).await;
            cmp_reader_file(Path::new("テスト.test"), &reader).await;
            cmp_reader_file(Path::new("subdir/ça bug.test"), &reader).await;
            cmp_reader_file(Path::new("subdir/sub_sub_dir/a.test"), &reader).await;
            cmp_reader_file(Path::new("subdir/sub_sub_dir/b.test"), &reader).await;
            cmp_reader_file(Path::new("subdir/sub_sub_dir/bin.test"), &reader).await;
        });
    }
}
