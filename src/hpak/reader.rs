use parking_lot::Mutex;
use std::{
    fs::File,
    io::{Cursor, Read, Seek, SeekFrom},
    ops::DerefMut,
    path::{Path, PathBuf},
    pin::Pin,
    task::{Context, Poll},
};

use bevy::{
    asset::io::{AssetReader, AssetReaderError},
    utils::HashMap,
};
use futures_lite::io::AsyncRead;

use crate::errors::Error;

use super::{compression::CompressionAlgorithm, encoder::Encoder, entry::Entry, header::Header};

pub struct HPakAssetsReader {
    source: Mutex<File>,
    header: Header,
    entries: HashMap<PathBuf, Entry>,
    directories: HashMap<PathBuf, Vec<PathBuf>>,
}

impl HPakAssetsReader {
    pub fn new(source: &Path) -> Result<Self, Error> {
        let mut source = std::fs::File::open(source)?;
        let mut entries = HashMap::new();
        let mut directories = HashMap::new();

        let header = Header::decode(&mut source)?;

        // go to the entry table
        source.seek(SeekFrom::Start(header.entry_table_offset))?;

        for (path, entry) in Vec::<(String, Entry)>::decode(&mut source)? {
            let path: PathBuf = path.into();

            let mut prev = path.clone();

            for ancestor in path.ancestors() {
                let ancestor: PathBuf = ancestor.into();

                let entries = directories.entry(ancestor.clone()).or_insert_with(Vec::new);

                if entries.iter().all(|path| *path != prev) {
                    entries.push(prev);
                }

                prev = ancestor;
            }

            entries.insert(path, entry);
        }

        Ok(Self {
            source: Mutex::new(source),
            header,
            entries,
            directories,
        })
    }

    fn read_entry(&self, path: &Path, is_meta: bool) -> Result<EntryReader, Error> {
        let entry = self.entries.get(path).ok_or(Error::NotFound)?;

        let (offset, length, compression_method) = if is_meta {
            (
                entry.offset,
                entry.meta_size,
                self.header.metadata_compression_method,
            )
        } else {
            (
                entry.offset + entry.meta_size,
                entry.data_size,
                entry.compression_method,
            )
        };

        let mut lock = self.source.lock();
        let source = lock.deref_mut();
        let mut raw = vec![0; (length - offset) as usize];

        source.seek(SeekFrom::Start(offset))?;
        source.take(length).read_to_end(&mut raw)?;

        Ok(EntryReader::new(raw, compression_method))
    }
}

impl AssetReader for HPakAssetsReader {
    fn read<'a>(
        &'a self,
        path: &'a Path,
    ) -> bevy::utils::BoxedFuture<'a, Result<Box<bevy::asset::io::Reader<'a>>, AssetReaderError>>
    {
        Box::pin(async move {
            self.read_entry(path, false).map_or_else(
                |err| Err(err.into_asset_reader_error(path)),
                |reader| Ok(Box::new(reader) as Box<bevy::asset::io::Reader<'a>>),
            )
        })
    }

    fn read_meta<'a>(
        &'a self,
        path: &'a Path,
    ) -> bevy::utils::BoxedFuture<'a, Result<Box<bevy::asset::io::Reader<'a>>, AssetReaderError>>
    {
        Box::pin(async move {
            self.read_entry(path, true).map_or_else(
                |err| Err(err.into_asset_reader_error(path)),
                |reader| Ok(Box::new(reader) as Box<bevy::asset::io::Reader<'a>>),
            )
        })
    }

    fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> bevy::utils::BoxedFuture<'a, Result<Box<bevy::asset::io::PathStream>, AssetReaderError>>
    {
        Box::pin(async move {
            self.directories.get(path).map_or_else(
                || Err(AssetReaderError::NotFound(path.to_path_buf())),
                |paths| Ok(Box::new(DirStream(paths.clone())) as Box<bevy::asset::io::PathStream>),
            )
        })
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> bevy::utils::BoxedFuture<'a, Result<bool, AssetReaderError>> {
        Box::pin(async move {
            let as_folder = path.join("");

            Ok(self
                .entries
                .keys()
                .any(|entry| entry != path && entry.starts_with(&as_folder)))
        })
    }
}

pub struct EntryReader(Box<dyn Read + Send + Sync>);

impl EntryReader {
    pub fn new(raw: Vec<u8>, compression_method: CompressionAlgorithm) -> Self {
        Self(match compression_method {
            CompressionAlgorithm::None => Box::new(Cursor::new(raw)),
            #[cfg(feature = "deflate")]
            CompressionAlgorithm::Deflate => {
                Box::new(flate2::read::DeflateDecoder::new(Cursor::new(raw)))
            }
            #[cfg(feature = "gzip")]
            CompressionAlgorithm::Gzip => Box::new(flate2::read::GzDecoder::new(Cursor::new(raw))),
            #[cfg(feature = "zlib")]
            CompressionAlgorithm::Zlib => {
                Box::new(flate2::read::ZlibDecoder::new(Cursor::new(raw)))
            }
            #[cfg(feature = "brotli")]
            CompressionAlgorithm::Brotli => {
                Box::new(brotli::Decompressor::new(Cursor::new(raw), 4096))
            }
        })
    }
}

impl AsyncRead for EntryReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buffer: &mut [u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        Poll::Ready(Ok(self.0.read(buffer)?))
    }
}

pub struct DirStream(Vec<PathBuf>);

impl futures_lite::Stream for DirStream {
    type Item = PathBuf;

    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.get_mut().0.pop())
    }
}
