use memmap2::Mmap;
use std::{
    fs::{File, OpenOptions},
    io::SeekFrom,
    path::{Path, PathBuf},
    pin::Pin,
    task::{Context, Poll},
};

use bevy::{
    asset::io::{AssetReader, AssetReaderError},
    render::render_resource::encase::internal::BufferRef,
    utils::{ConditionalSendFuture, HashMap},
};
use futures_io::AsyncSeek;
use futures_lite::{
    io::{AsyncRead, BufReader, Cursor},
    Future,
};

use crate::{errors::Error, CompressionAlgorithm, Decode};

use super::{entry::Entry, header::Header};

pub struct HPakAssetsReader {
    source_file: File,
    source: Mmap,
    header: Header,
    entries: HashMap<PathBuf, Entry>,
    directories: HashMap<PathBuf, Vec<PathBuf>>,
}

impl HPakAssetsReader {
    pub fn new(path: &Path) -> Result<Self, Error> {
        let mut source = OpenOptions::new().read(true).open(path)?;
        let mut entries = HashMap::new();
        let mut directories = HashMap::new();

        let header = Header::decode(&mut source)?;

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

        let source_map = unsafe { Mmap::map(&source)? };

        Ok(Self {
            source_file: source,
            source: source_map,
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

        Ok(EntryReader::new(
            &self.source,
            offset,
            length,
            compression_method,
        ))
    }
}

impl AssetReader for HPakAssetsReader {
    fn read<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl ConditionalSendFuture<Output = Result<Box<bevy::asset::io::Reader<'a>>, AssetReaderError>>
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
    ) -> impl ConditionalSendFuture<Output = Result<Box<bevy::asset::io::Reader<'a>>, AssetReaderError>>
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
    ) -> impl ConditionalSendFuture<Output = Result<Box<bevy::asset::io::PathStream>, AssetReaderError>>
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
    ) -> impl ConditionalSendFuture + Future<Output = Result<bool, bevy::asset::io::AssetReaderError>>
    {
        Box::pin(async move {
            let as_folder = path.join("");

            Ok(self
                .entries
                .keys()
                .any(|entry| entry != path && entry.starts_with(&as_folder)))
        })
    }
}

pub trait AsyncReadPosition: AsyncRead + Position {}

impl<T: AsyncRead + Position> AsyncReadPosition for T {}

pub struct EntryReader {
    reader: Box<dyn AsyncReadPosition + Unpin + Send + Sync>,
}

impl EntryReader {
    #[inline]
    pub fn new(
        source: &Mmap,
        offset: u64,
        length: u64,
        compression_method: CompressionAlgorithm,
    ) -> Self {
        let mut buffer = Vec::with_capacity(length as usize);
        source.read_slice(offset as usize, &mut buffer);

        Self {
            reader: match compression_method {
                CompressionAlgorithm::None => Box::new(Cursor::new(buffer)),
                CompressionAlgorithm::Deflate => {
                    let reader = BufReader::new(Cursor::new(buffer));
                    Box::new(async_compression::futures::bufread::DeflateDecoder::new(
                        reader,
                    ))
                }
            },
        }
    }
}

pub trait Position {
    fn position(&self) -> u64;
}

impl<T: Position> Position for Box<T> {
    fn position(&self) -> u64 {
        (**self).position()
    }
}

impl Position for Cursor<Vec<u8>> {
    fn position(&self) -> u64 {
        self.position()
    }
}

impl Position for async_compression::futures::bufread::DeflateDecoder<BufReader<Cursor<Vec<u8>>>> {
    fn position(&self) -> u64 {
        self.get_ref().position()
    }
}

impl Position for BufReader<Cursor<Vec<u8>>> {
    fn position(&self) -> u64 {
        self.get_ref().position()
    }
}

impl AsyncRead for EntryReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.reader).poll_read(cx, buffer)
    }
}

impl AsyncSeek for EntryReader {
    fn poll_seek(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        pos: SeekFrom,
    ) -> Poll<Result<u64, std::io::Error>> {
        match pos {
            SeekFrom::Current(pos) if pos >= 0 => {
                let mut phantom_buffer = Vec::with_capacity(pos as usize);

                match Pin::new(&mut self.reader).poll_read(cx, &mut phantom_buffer) {
                    Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
                    Poll::Ready(Ok(_)) => Poll::Ready(Ok(self.reader.position())),
                    Poll::Pending => Poll::Pending,
                }
            }
            _ => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "only seeking forward from current position is supported",
            ))),
        }
    }
}

pub struct DirStream(Vec<PathBuf>);

impl futures_lite::Stream for DirStream {
    type Item = PathBuf;

    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.get_mut().0.pop())
    }
}
