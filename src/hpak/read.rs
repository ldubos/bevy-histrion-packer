use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

#[cfg(target_family = "unix")]
use std::os::unix::fs::FileExt;
#[cfg(target_family = "windows")]
use std::os::windows::fs::FileExt;

use bevy::asset::io::PathStream;

use super::{encoder::Encoder, header::Header};

pub struct HPakReader {
    source: Arc<File>,
    header: Header,
}

impl HPakReader {
    pub fn new(source: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let mut source = File::open(source)?;
        let header = Header::decode(&mut source)?;

        Ok(Self {
            source: Arc::new(source),
            header,
        })
    }

    pub fn read_meta(&self, path: &Path) -> Result<HPakEntryReader, Box<dyn std::error::Error>> {
        if let Some(entry) = self.header.entries.iter().find(|e| e.path == path) {
            Ok(HPakEntryReader::new(
                self.source.clone(),
                entry.offset,
                entry.meta_size,
            ))
        } else {
            Err("entry not found".into())
        }
    }

    pub fn read_data(&self, path: &Path) -> Result<HPakEntryReader, Box<dyn std::error::Error>> {
        if let Some(entry) = self.header.entries.iter().find(|e| e.path == path) {
            Ok(HPakEntryReader::new(
                self.source.clone(),
                entry.offset + entry.meta_size,
                entry.data_size,
            ))
        } else {
            Err("entry not found".into())
        }
    }

    pub fn read_directory(
        &self,
        path: &Path,
    ) -> Result<Box<PathStream>, Box<dyn std::error::Error>> {
        if !self.is_directory(path) {
            return Err("not a directory".into());
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

#[cfg(target_family = "unix")]
fn read_at(file: &File, offset: u64, buffer: &mut [u8]) -> std::io::Result<usize> {
    file.read_at(buffer, offset)
}

#[cfg(target_family = "windows")]
fn read_at(file: &File, offset: u64, buffer: &mut [u8]) -> std::io::Result<usize> {
    file.seek_read(buffer, offset)
}

pub struct PartialReader {
    file: Arc<File>,
    offset: u64,
    cursor: u64,
    length: u64,
}

impl PartialReader {
    pub fn new(file: Arc<File>, offset: u64, length: u64) -> Self {
        Self {
            file,
            offset,
            length,
            cursor: 0,
        }
    }
}

impl std::io::Read for PartialReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let to_read = std::cmp::min(buf.len(), (self.length - self.cursor) as usize);

        if to_read == 0 {
            return Ok(0);
        }

        let read = read_at(&self.file, self.offset + self.cursor, &mut buf[..to_read])?;
        self.cursor += read as u64;
        Ok(read)
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
