use super::*;
use crate::{Error, Result, encoding::*};
use bevy::asset::io::{
    AssetReader, AssetReaderError, PathStream, Reader, ReaderRequiredFeatures, SeekKind,
    UnsupportedReaderFeature,
};
use futures_io::{AsyncRead, AsyncSeek};
use memmap2::Mmap;
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
    /// Create a new HPAK reader for the archive at the specified path.
    ///
    /// This opens the file and reads the header and entry table into memory.
    /// The actual asset data remains on disk and is accessed via memory mapping.
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let mut file = open_archive(path)?;

        // read header
        file.seek(SeekFrom::Start(0))?;
        let header = HpakHeader::decode(&mut file)?;

        // jump to the entries table
        file.seek(SeekFrom::Start(header.entries_offset))?;
        let entries = HpakEntries::decode(&mut file)?;

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

        self.entries
            .files
            .find(hash, |entry| entry.hash == hash)
            .ok_or_else(|| Error::EntryNotFound(path.to_path_buf()))
    }
}

impl AssetReader for HpakReader {
    async fn read<'a>(
        &'a self,
        path: &'a Path,
        required_features: ReaderRequiredFeatures,
    ) -> std::result::Result<Box<dyn Reader + 'a>, AssetReaderError> {
        match required_features.seek {
            SeekKind::OnlyForward => { /* ok */ }
            SeekKind::AnySeek => {
                return Err(AssetReaderError::UnsupportedFeature(
                    UnsupportedReaderFeature::AnySeek,
                ));
            }
        }

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

    async fn read_meta_bytes<'a>(
        &'a self,
        path: &'a Path,
    ) -> std::result::Result<Vec<u8>, AssetReaderError> {
        let entry = self.get_entry(path)?;

        if entry.meta_size == 0 {
            return Ok(Vec::new());
        }

        let start = entry.meta_offset as usize;
        let end = start + entry.meta_size as usize;

        match entry.compression_method {
            CompressionMethod::None => Ok(self.mmap[start..end].to_vec()),
            CompressionMethod::Zlib => {
                let mut meta_reader = self.read_meta(path)?;
                let mut meta_bytes = Vec::new();
                meta_reader.read_to_end(&mut meta_bytes).await?;
                Ok(meta_bytes)
            }
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

impl HpakEntryReader {
    pub fn new(
        source: Arc<Mmap>,
        offset: u64,
        size: u64,
        compression_method: CompressionMethod,
    ) -> Self {
        let slice = MmapSliceReader::new(source.clone(), offset as usize, size as usize);

        let state = match compression_method {
            CompressionMethod::None => ReaderState::Uncompressed(slice),
            CompressionMethod::Zlib => ReaderState::Compressed {
                cursor: 0,
                decoder: Box::new(flate2::read::ZlibDecoder::new_with_buf(
                    slice,
                    vec![0u8; 4 * 1024],
                )) as Box<dyn Read + Send + Sync>,
            },
        };

        Self { state }
    }
}

enum ReaderState {
    Uncompressed(MmapSliceReader),
    Compressed {
        cursor: u64,
        decoder: Box<dyn Read + Send + Sync + 'static>,
    },
}

struct MmapSliceReader {
    source: Arc<Mmap>,
    offset: usize,
    len: usize,
    pos: usize,
}

impl MmapSliceReader {
    fn new(source: Arc<Mmap>, offset: usize, len: usize) -> Self {
        Self {
            source,
            offset,
            len,
            pos: 0,
        }
    }

    fn seek_forward(&mut self, offset: u64) -> std::io::Result<u64> {
        let new_pos = self.pos as u64 + offset;

        if new_pos > self.len as u64 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek out of bounds",
            ));
        }

        self.pos = new_pos as usize;
        Ok(self.pos as u64)
    }
}

#[cold]
#[inline(always)]
fn cold() {}

/// Hint the compiler that it is unlikely to be true.
#[inline(always)]
fn unlikely(b: bool) -> bool {
    if b {
        cold();
    }
    b
}

impl Read for MmapSliceReader {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if unlikely(buf.is_empty() || self.pos >= self.len) {
            return Ok(0);
        }

        let remaining = self.len - self.pos;
        let to_read = remaining.min(buf.len());
        let start = self.offset + self.pos;

        let src = &self.source[..];

        // SAFETY: We have ensured that the ranges are valid and non-overlapping,
        // and `buf` is not part of the mmap
        unsafe {
            core::ptr::copy_nonoverlapping(src.as_ptr().add(start), buf.as_mut_ptr(), to_read);
        }

        self.pos += to_read;

        Ok(to_read)
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
            ReaderState::Compressed { cursor, decoder } => match decoder.read(buf) {
                Ok(0) => Poll::Ready(Ok(0)),
                Ok(n) => {
                    *cursor += n as u64;
                    Poll::Ready(Ok(n))
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Poll::Pending,
                Err(e) => Poll::Ready(Err(e)),
            },
        }
    }
}

impl AsyncSeek for HpakEntryReader {
    fn poll_seek(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        pos: SeekFrom,
    ) -> Poll<futures_io::Result<u64>> {
        let offset = match pos {
            SeekFrom::Current(offset) => {
                if unlikely(offset < 0) {
                    return Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::Unsupported,
                        "backward seeking is not supported",
                    )));
                }
                offset as u64
            }
            _ => {
                return Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "only forward seeking is supported",
                )));
            }
        };

        match &mut self.state {
            ReaderState::Uncompressed(cursor) => match cursor.seek_forward(offset) {
                Ok(new_pos) => Poll::Ready(Ok(new_pos)),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Poll::Pending,
                Err(e) => Poll::Ready(Err(e)),
            },
            ReaderState::Compressed { cursor, decoder } => {
                let mut offset = offset;
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
    use futures::{AsyncSeekExt, executor::block_on};
    use rstest::rstest;

    #[rstest]
    #[case("test.png", CompressionMethod::None)]
    #[case("test.png.zlib", CompressionMethod::Zlib)]
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
    #[case("test.png.zlib", CompressionMethod::Zlib)]
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
            assert_eq!(1024, reader.seek(SeekFrom::Current(1024)).await.unwrap());
            assert_eq!(
                1024 + 8192,
                reader.seek(SeekFrom::Current(8192)).await.unwrap()
            );
            reader.read_to_end(&mut buffer).await.unwrap();
        });

        assert_eq!(base[(1024 + 8192)..], buffer);
    }
}
