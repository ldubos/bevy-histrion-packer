use std::path::Path;

use bevy::asset::io::AssetReader;

use crate::hpak::HPakReader;

pub struct HistrionPakAssetsReader {
    reader: HPakReader,
}

impl HistrionPakAssetsReader {
    pub fn new(source: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            reader: HPakReader::new(source)?,
        })
    }
}

impl AssetReader for HistrionPakAssetsReader {
    fn read<'a>(
        &'a self,
        path: &'a Path,
    ) -> bevy::utils::BoxedFuture<
        'a,
        Result<Box<bevy::asset::io::Reader<'a>>, bevy::asset::io::AssetReaderError>,
    > {
        Box::pin(async move {
            self.reader
                .read_data(path)
                .map(|r| Box::new(r) as Box<bevy::asset::io::Reader<'a>>)
                .or(Err(bevy::asset::io::AssetReaderError::NotFound(
                    path.to_path_buf(),
                )))
        })
    }

    fn read_meta<'a>(
        &'a self,
        path: &'a Path,
    ) -> bevy::utils::BoxedFuture<
        'a,
        Result<Box<bevy::asset::io::Reader<'a>>, bevy::asset::io::AssetReaderError>,
    > {
        Box::pin(async move {
            self.reader
                .read_meta(path)
                .map(|r| Box::new(r) as Box<bevy::asset::io::Reader<'a>>)
                .or(Err(bevy::asset::io::AssetReaderError::NotFound(
                    path.to_path_buf(),
                )))
        })
    }

    fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> bevy::utils::BoxedFuture<
        'a,
        Result<Box<bevy::asset::io::PathStream>, bevy::asset::io::AssetReaderError>,
    > {
        Box::pin(async move {
            self.reader
                .read_directory(path)
                .map(|r| Box::new(r) as Box<bevy::asset::io::PathStream>)
                .or(Err(bevy::asset::io::AssetReaderError::NotFound(
                    path.to_path_buf(),
                )))
        })
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> bevy::utils::BoxedFuture<'a, Result<bool, bevy::asset::io::AssetReaderError>> {
        Box::pin(async move { Ok(self.reader.is_directory(path)) })
    }
}
