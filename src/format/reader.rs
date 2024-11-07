use super::*;
use crate::{encoding::*, Error, Result};
use bevy::asset::io::{AssetReader, AssetReaderError, AsyncSeekForward, PathStream, Reader};
use futures_io::AsyncRead;
use memmap2::Mmap;
use std::io::Cursor;
use std::mem::ManuallyDrop;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{
    fs::{File, OpenOptions},
    io::{Seek, SeekFrom},
    sync::Arc,
};

pub struct HpakReader {
    file: ManuallyDrop<File>,
    mmap: Arc<Mmap>,
    meta_compression_method: CompressionMethod,
    entries: HpakEntries,
}

impl Drop for HpakReader {
    fn drop(&mut self) {
        unsafe {
            ManuallyDrop::drop(&mut self.file);
        }
    }
}

const _: () = {
    _assert_send::<HpakReader>();
    _assert_sync::<HpakReader>();
};

#[cfg(windows)]
fn open_archive(path: impl AsRef<Path>) -> Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    Ok(OpenOptions::new()
        .read(true)
        .share_mode(0x00000001 /*FILE_SHARE_READ*/)
        .custom_flags(0x10000000 /*FILE_FLAG_RANDOM_ACCESS*/)
        .open(path)?)
}

#[cfg(unix)]
fn open_archive(path: impl AsRef<Path>) -> Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    Ok(OpenOptions::new()
        .read(true)
        .custom_flags(0o1000000 /*O_NOATIME*/)
        .open(path)?)
}

impl HpakReader {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let mut file = open_archive(path)?;

        // read header
        file.seek(SeekFrom::Start(0))?;
        let header = HpakHeader::decode(&mut file)?;

        // jump to the entries table
        file.seek(SeekFrom::Start(header.entries_offset))?;
        let entries = HpakEntries::decode(&mut file)?;

        println!("{:#?}", entries);

        let mmap = unsafe { Mmap::map(&file)? };

        Ok(Self {
            file: ManuallyDrop::new(file),
            mmap: Arc::new(mmap),
            meta_compression_method: header.meta_compression_method,
            entries,
        })
    }

    pub fn read_meta(&self, path: &Path) -> Result<HpakEntryReader> {
        let entry = self.get_entry(path)?;

        Ok(HpakEntryReader::new(
            self.mmap.clone(),
            entry.meta_offset,
            entry.meta_size,
            self.meta_compression_method,
        ))
    }

    pub fn read_data(&self, path: &Path) -> Result<HpakEntryReader> {
        let entry = self.get_entry(path)?;

        Ok(HpakEntryReader::new(
            self.mmap.clone(),
            entry.meta_offset + entry.meta_size,
            entry.data_size,
            entry.compression_method,
        ))
    }

    fn get_entry(&self, path: &Path) -> Result<&HpakFileEntry> {
        let hash = hash_path(path);

        println!("hash: {}", hash);

        self.entries
            .files
            .find(hash, |entry| entry.hash == hash)
            .map_or(Err(Error::EntryNotFound(path.to_path_buf())), |entry| {
                Ok(entry)
            })
    }
}

impl AssetReader for HpakReader {
    async fn read<'a>(
        &'a self,
        path: &'a Path,
    ) -> std::result::Result<Box<dyn Reader + 'a>, AssetReaderError> {
        match self.read_data(path) {
            Ok(reader) => Ok(Box::new(reader)),
            Err(e) => Err(e.into()),
        }
    }

    async fn read_meta<'a>(
        &'a self,
        path: &'a Path,
    ) -> std::result::Result<Box<dyn Reader + 'a>, AssetReaderError> {
        match self.read_meta(path) {
            Ok(reader) => Ok(Box::new(reader)),
            Err(e) => Err(e.into()),
        }
    }

    async fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> std::result::Result<Box<PathStream>, AssetReaderError> {
        let hash = hash_path(path);

        match self
            .entries
            .directories
            .find(hash, |entry| entry.hash == hash)
        {
            Some(entry) => Ok(Box::new(DirStream(entry.entries.clone()))),
            None => Err(AssetReaderError::NotFound(path.to_path_buf())),
        }
    }

    async fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> std::result::Result<bool, bevy::asset::io::AssetReaderError> {
        let path = hash_path(path);

        Ok(self
            .entries
            .directories
            .find(path, |entry| entry.hash == path)
            .is_some())
    }
}

pub struct HpakEntryReader {
    state: ReaderState,
}

enum ReaderState {
    Uncompressed(Cursor<Vec<u8>>),
    #[cfg(feature = "deflate")]
    Compressed {
        cursor: u64,
        decoder: Arc<parking_lot::Mutex<dyn Read + Send + Sync + 'static>>,
    },
}

impl HpakEntryReader {
    pub fn new(
        source: Arc<Mmap>,
        offset: u64,
        size: u64,
        compression_method: CompressionMethod,
    ) -> Self {
        let slice = Cursor::new(source[offset as usize..(offset + size) as usize].to_owned());

        let state = match compression_method {
            CompressionMethod::None => ReaderState::Uncompressed(slice),
            #[cfg(feature = "deflate")]
            CompressionMethod::Deflate => ReaderState::Compressed {
                cursor: 0,
                decoder: Arc::new(parking_lot::Mutex::new(Box::new(
                    flate2::read::DeflateDecoder::new_with_buf(slice, vec![0u8; 4 * 1024]),
                ))),
            },
        };

        Self { state }
    }
}

impl AsyncRead for HpakEntryReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        match &mut self.state {
            ReaderState::Uncompressed(cursor) => match cursor.read(buf) {
                Ok(n) => Poll::Ready(Ok(n)),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Poll::Pending,
                Err(e) => Poll::Ready(Err(e)),
            },
            #[cfg(feature = "deflate")]
            ReaderState::Compressed { cursor, decoder } => {
                let mut decoder = decoder.lock();
                match decoder.read(buf) {
                    Ok(0) => Poll::Ready(Ok(0)),
                    Ok(n) => {
                        *cursor += n as u64;
                        Poll::Ready(Ok(n))
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Poll::Pending,
                    Err(e) => Poll::Ready(Err(e)),
                }
            }
        }
    }
}

impl AsyncSeekForward for HpakEntryReader {
    fn poll_seek_forward(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        offset: u64,
    ) -> Poll<futures_io::Result<u64>> {
        match &mut self.state {
            ReaderState::Uncompressed(cursor) => {
                match cursor.seek(SeekFrom::Current(offset as i64)) {
                    Ok(new_pos) => Poll::Ready(Ok(new_pos)),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Poll::Pending,
                    Err(e) => Poll::Ready(Err(e)),
                }
            }
            #[cfg(feature = "deflate")]
            ReaderState::Compressed { cursor, decoder } => {
                let mut offset = offset;
                let mut decoder = decoder.lock();
                let mut read_buffer = vec![0u8; 4096.min(offset as usize)];

                while offset > 0 {
                    match decoder.read(&mut read_buffer[..offset.min(4096) as usize]) {
                        Ok(0) => break,
                        Ok(n) => {
                            *cursor += n as u64;
                            offset -= n as u64;
                        }
                        Err(e) => return Poll::Ready(Err(e)),
                    }
                }

                Poll::Ready(Ok(*cursor))
            }
        }
    }
}

impl Reader for HpakEntryReader {}

pub struct DirStream(Vec<PathBuf>);

impl futures_lite::Stream for DirStream {
    type Item = PathBuf;

    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.get_mut().0.pop())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::io::AsyncSeekForwardExt;
    use futures::executor::block_on;
    use rstest::rstest;

    #[rstest]
    #[case("test.png", CompressionMethod::None)]
    #[cfg_attr(
        feature = "deflate",
        case("test.png.deflate", CompressionMethod::Deflate)
    )]
    fn it_read_entry(#[case] name: &str, #[case] compression_method: CompressionMethod) {
        let uncompressed =
            std::fs::read(format!("{}/fuzz/test.png", env!("CARGO_MANIFEST_DIR"),)).unwrap();

        let compressed =
            File::open(format!("{}/fuzz/{}", env!("CARGO_MANIFEST_DIR"), name)).unwrap();

        let mmap = unsafe { Mmap::map(&compressed).unwrap() };

        let mut reader = HpakEntryReader::new(
            Arc::new(mmap),
            0,
            compressed.metadata().unwrap().len() as u64,
            compression_method,
        );

        let mut buffer = Vec::new();

        block_on(async { reader.read_to_end(&mut buffer).await.unwrap() });

        assert_eq!(uncompressed, buffer);
    }

    #[rstest]
    #[case("test.png", CompressionMethod::None)]
    #[cfg_attr(
        feature = "deflate",
        case("test.png.deflate", CompressionMethod::Deflate)
    )]
    fn it_seek_entry(#[case] name: &str, #[case] compression_method: CompressionMethod) {
        let base = std::fs::read(format!("{}/fuzz/test.png", env!("CARGO_MANIFEST_DIR"),)).unwrap();

        let encoded = File::open(format!("{}/fuzz/{}", env!("CARGO_MANIFEST_DIR"), name)).unwrap();

        let mmap = unsafe { Mmap::map(&encoded).unwrap() };

        let mut reader = HpakEntryReader::new(
            Arc::new(mmap),
            0,
            encoded.metadata().unwrap().len() as u64,
            compression_method,
        );

        let mut buffer = Vec::new();

        block_on(async {
            assert_eq!(1024, reader.seek_forward(1024).await.unwrap());
            assert_eq!(1024 + 8192, reader.seek_forward(8192).await.unwrap());
            reader.read_to_end(&mut buffer).await.unwrap();
        });

        assert_eq!(base[(1024 + 8192)..], buffer);
    }
}
