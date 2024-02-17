use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use crate::HPakError;

use super::{encoder::Encoder, header::Header};

pub struct HPakReader {
    source: Arc<File>,
    header: Header,
}

impl HPakReader {
    pub fn new(source: &Path) -> Result<Self, HPakError> {
        let mut source = File::open(source)?;
        let header = Header::decode(&mut source)?;

        Ok(Self {
            source: Arc::new(source),
            header,
        })
    }

    fn read_entry(&self, path: &Path, is_meta: bool) -> Result<HPakEntryReader, HPakError> {
        if let Some(entry) = self.header.entries.iter().find(|e| e.path == path) {
            let offset = if is_meta {
                entry.offset
            } else {
                entry.offset + entry.meta_size
            };
            let length = if is_meta {
                entry.meta_size
            } else {
                entry.data_size
            };

            Ok(HPakEntryReader::new(self.source.clone(), offset, length))
        } else {
            Err(HPakError::NotFound)
        }
    }

    pub fn read_meta(&self, path: &Path) -> Result<HPakEntryReader, HPakError> {
        self.read_entry(path, true)
    }

    pub fn read_data(&self, path: &Path) -> Result<HPakEntryReader, HPakError> {
        self.read_entry(path, false)
    }

    pub fn read_directory(
        &self,
        path: &Path,
    ) -> Result<Box<bevy::asset::io::PathStream>, HPakError> {
        if !self.is_directory(path) {
            return Err(HPakError::NotFound);
        }

        let mut paths: Vec<PathBuf> = self
            .header
            .entries
            .iter()
            .filter_map(|entry| {
                if entry.path.starts_with(path) {
                    let relative_path = entry.path.strip_prefix(path).ok()?;

                    match relative_path.components().next() {
                        Some(std::path::Component::Normal(first)) => Some(path.join(first)),
                        _ => None,
                    }
                } else {
                    None
                }
            })
            .collect();

        paths.dedup();

        Ok(Box::new(DirStream(paths.clone())))
    }

    pub fn is_directory(&self, path: &Path) -> bool {
        let as_folder = path.join("");
        self.header
            .entries
            .iter()
            .any(|entry| entry.path.starts_with(&as_folder) && entry.path != path)
    }
}

/// A reader that allows reading parts of a file.
///
/// It is designed to read a specific segment of a file and permits multi-threaded access to a file by using
/// [`std::os::unix::fs::FileExt::read_at`] on Unix systems and [`std::os::windows::fs::FileExt::seek_read`] on Windows.
pub struct PartialReader {
    file: Arc<File>,
    cursor: u64,
    length: u64,
}

impl PartialReader {
    pub fn new(file: Arc<File>, offset: u64, length: u64) -> Self {
        Self {
            file,
            cursor: offset,
            length: length + offset,
        }
    }
}

impl std::io::Read for PartialReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let max_bytes = buf.len().min(
            (std::num::Saturating(self.length) - std::num::Saturating(self.cursor)).0 as usize,
        );

        if max_bytes == 0 {
            return Ok(0);
        }

        let bytes_red = {
            #[cfg(target_family = "unix")]
            {
                use std::os::unix::fs::FileExt;
                self.file.read_at(&mut buf[..max_bytes], self.cursor)?
            }

            #[cfg(target_family = "windows")]
            {
                use std::os::windows::fs::FileExt;
                self.file.seek_read(&mut buf[..max_bytes], self.cursor)?
            }

            #[cfg(not(any(target_family = "unix", target_family = "windows")))]
            {
                panic!("unsupported platform");
            }
        };

        self.cursor += bytes_red as u64;
        Ok(bytes_red)
    }
}

pub struct HPakEntryReader {
    decoder: flate2::read::ZlibDecoder<PartialReader>,
}

impl HPakEntryReader {
    pub fn new(source: Arc<File>, offset: u64, length: u64) -> Self {
        let reader = PartialReader::new(source, offset, length);

        Self {
            decoder: flate2::read::ZlibDecoder::new(reader),
        }
    }
}

impl futures_io::AsyncRead for HPakEntryReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buffer: &mut [u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        Poll::Ready(self.decoder.read(buffer))
    }
}

pub struct DirStream(Vec<PathBuf>);

impl futures_lite::Stream for DirStream {
    type Item = PathBuf;

    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.get_mut().0.pop())
    }
}
