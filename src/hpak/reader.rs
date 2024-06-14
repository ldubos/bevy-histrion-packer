use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use bevy::{
    asset::io::{AssetReader, AssetReaderError},
    utils::{ConditionalSendFuture, HashMap},
};
use blocking::Task;
use futures_io::AsyncSeek;
use futures_lite::{io::AsyncRead, ready, Future, FutureExt};

use crate::{errors::Error, CompressionAlgorithm};

use super::{encoder::Encoder, entry::Entry, header::Header};

pub struct HPakAssetsReader {
    source: Arc<File>,
    header: Header,
    entries: HashMap<PathBuf, Entry>,
    directories: HashMap<PathBuf, Vec<PathBuf>>,
}

impl HPakAssetsReader {
    pub fn new(path: &Path) -> Result<Self, Error> {
        let mut source = {
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;

                OpenOptions::new()
                    .custom_flags(0x10000000 /* FILE_FLAG_RANDOM_ACCESS */)
                    .read(true)
                    .open(path)?
            }

            #[cfg(unix)]
            OpenOptions::new().read(true).open(path)?
        };
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
            source: Arc::new(source),
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
            self.source.clone(),
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

pub enum EntryState {
    Busy(Task<std::io::Result<Vec<u8>>>),
    Reading(std::io::Cursor<Vec<u8>>),
}

pub struct EntryReader(EntryState);

impl EntryReader {
    #[inline]
    pub fn new(
        file: Arc<File>,
        offset: u64,
        length: u64,
        compression_method: CompressionAlgorithm,
    ) -> Self {
        let file = file.clone();

        Self(EntryState::Busy(blocking::unblock(move || {
            let mut buffer = vec![0; length as usize];
            read_exact_at(&file, &mut buffer, offset)?;

            let buffer = match compression_method {
                CompressionAlgorithm::None => buffer,
                #[cfg(feature = "deflate")]
                CompressionAlgorithm::Deflate => {
                    let mut decoded = Vec::new();
                    let mut decoder =
                        flate2::read::DeflateDecoder::new(std::io::Cursor::new(&buffer));
                    decoder.read_to_end(&mut decoded)?;
                    decoded
                }
                #[cfg(feature = "brotli")]
                CompressionAlgorithm::Brotli => {
                    let mut decoded = Vec::new();
                    let mut decoder =
                        brotli::Decompressor::new(std::io::Cursor::new(&buffer), 4096);
                    decoder.read_to_end(&mut decoded)?;
                    decoded
                }
            };

            Ok(buffer)
        })))
    }
}

impl AsyncRead for EntryReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        loop {
            match &mut self.0 {
                EntryState::Busy(task) => {
                    let result = ready!(task.poll(cx)?);
                    self.0 = EntryState::Reading(std::io::Cursor::new(result));
                }
                EntryState::Reading(cursor) => {
                    let n = cursor.read(buffer)?;
                    return Poll::Ready(Ok(n));
                }
            }
        }
    }
}

impl AsyncSeek for EntryReader {
    fn poll_seek(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        pos: SeekFrom,
    ) -> Poll<Result<u64, std::io::Error>> {
        loop {
            match &mut self.0 {
                EntryState::Busy(task) => {
                    let result = ready!(task.poll(cx)?);
                    self.0 = EntryState::Reading(std::io::Cursor::new(result));
                }
                EntryState::Reading(cursor) => {
                    let n = cursor.seek(pos)?;
                    return Poll::Ready(Ok(n));
                }
            }
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

#[inline]
#[cfg(windows)]
fn read_exact_at(file: &File, mut buf: &mut [u8], mut offset: u64) -> std::io::Result<()> {
    use std::os::windows::fs::FileExt;

    while !buf.is_empty() {
        match file.seek_read(&mut buf, offset) {
            Ok(0) => break,
            Ok(n) => {
                buf = &mut buf[n..];
                offset += n as u64;
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }

    if !buf.is_empty() {
        Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "failed to fill whole buffer",
        ))
    } else {
        Ok(())
    }
}

#[inline]
#[cfg(unix)]
fn read_exact_at(file: &File, buf: &mut [u8], offset: u64) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.read_exact_at(buf, offset)
}
