mod reader;
mod writer;

use std::{
    hash::{Hash, Hasher},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use bevy::utils::{hashbrown::HashTable, AHasher};

use crate::{encoding::*, Result};

pub use reader::*;
pub use writer::*;

#[derive(Debug, Clone)]
pub struct HpakHeader {
    /// Metadata compression method.
    pub(crate) meta_compression_method: CompressionMethod,
    /// Offset of the entry table in the archive.
    pub(crate) entries_offset: u64,
}

impl Encode for HpakHeader {
    fn encode<W: Write>(&self, mut writer: W) -> crate::Result<usize> {
        Ok(crate::MAGIC.encode(&mut writer)?
            + crate::VERSION.encode(&mut writer)?
            + self.meta_compression_method.encode(&mut writer)?
            + self.entries_offset.encode(&mut writer)?)
    }
}

impl Decode for HpakHeader {
    fn decode<R: std::io::Read>(mut reader: R) -> crate::Result<Self> {
        let magic = <[u8; 4]>::decode(&mut reader)?;

        if magic != crate::MAGIC {
            return Err(crate::Error::InvalidFileFormat);
        }

        let version = u32::decode(&mut reader)?;

        if version != crate::VERSION {
            return Err(crate::Error::BadVersion(version));
        }

        Ok(Self {
            meta_compression_method: CompressionMethod::decode(&mut reader)?,
            entries_offset: u64::decode(&mut reader)?,
        })
    }
}

/// An entry in the HPAK archive.
#[derive(Debug, Clone)]
pub struct HpakFileEntry {
    /// Hash of the entry's path.
    pub(crate) hash: u64,
    /// Data compression method.
    pub(crate) compression_method: CompressionMethod,
    /// Offset of the metadata in the archive.
    pub(crate) meta_offset: u64,
    /// Size of the metadata.
    pub(crate) meta_size: u64,
    /// Size of the data. Data is located after the metadata.
    pub(crate) data_size: u64,
}

impl HpakFileEntry {
    pub const fn hash(&self) -> u64 {
        self.hash
    }
}

impl Encode for HpakFileEntry {
    fn encode<W: Write>(&self, mut writer: W) -> crate::Result<usize> {
        Ok(self.hash.encode(&mut writer)?
            + self.compression_method.encode(&mut writer)?
            + self.meta_offset.encode(&mut writer)?
            + self.meta_size.encode(&mut writer)?
            + self.data_size.encode(&mut writer)?)
    }
}

impl Decode for HpakFileEntry {
    fn decode<R: std::io::Read>(mut reader: R) -> crate::Result<Self> {
        Ok(Self {
            hash: u64::decode(&mut reader)?,
            compression_method: CompressionMethod::decode(&mut reader)?,
            meta_offset: u64::decode(&mut reader)?,
            meta_size: u64::decode(&mut reader)?,
            data_size: u64::decode(&mut reader)?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct HpakDirectoryEntry {
    /// Hash of the entry's path.
    pub(crate) hash: u64,
    /// Entries present in this directory.
    pub(crate) entries: Vec<PathBuf>,
}

impl HpakDirectoryEntry {
    pub const fn hash(&self) -> u64 {
        self.hash
    }
}

impl Encode for HpakDirectoryEntry {
    fn encode<W: Write>(&self, mut writer: W) -> crate::Result<usize> {
        Ok(self.hash.encode(&mut writer)? + self.entries.encode(&mut writer)?)
    }
}

impl Decode for HpakDirectoryEntry {
    fn decode<R: std::io::Read>(mut reader: R) -> crate::Result<Self> {
        Ok(Self {
            hash: u64::decode(&mut reader)?,
            entries: Vec::<PathBuf>::decode(&mut reader)?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct HpakEntries {
    /// File entries in the archive.
    pub(crate) files: HashTable<HpakFileEntry>,
    /// Directory entries in the archive.
    pub(crate) directories: HashTable<HpakDirectoryEntry>,
}

impl Encode for HpakEntries {
    fn encode<W: Write>(&self, mut writer: W) -> Result<usize> {
        let directories_len = (self.directories.len() as u64).encode(&mut writer)?;
        let directories = self.directories.iter().try_fold(0usize, |acc, v| {
            Ok(v.encode(&mut writer)? + acc) as Result<usize>
        })?;

        let entries_len = (self.files.len() as u64).encode(&mut writer)?;
        let entries = self.files.iter().try_fold(0usize, |acc, v| {
            Ok(v.encode(&mut writer)? + acc) as Result<usize>
        })?;

        Ok(directories_len + directories + entries_len + entries)
    }
}

impl Decode for HpakEntries {
    fn decode<R: Read>(mut reader: R) -> Result<Self> {
        let directories_len = u64::decode(&mut reader)?;
        let mut directories = HashTable::with_capacity(directories_len as usize);

        for _ in 0..directories_len {
            let entry = HpakDirectoryEntry::decode(&mut reader)?;
            directories.insert_unique(entry.hash, entry, HpakDirectoryEntry::hash);
        }

        let entries_len = u64::decode(&mut reader)?;
        let mut entries = HashTable::with_capacity(entries_len as usize);

        for _ in 0..entries_len {
            let entry = HpakFileEntry::decode(&mut reader)?;
            entries.insert_unique(entry.hash, entry, HpakFileEntry::hash);
        }

        Ok(Self {
            directories,
            files: entries,
        })
    }
}

#[repr(u8)]
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompressionMethod {
    #[default]
    None = 0,
    /// Deflate(level), level is a number between 0 and 9
    Deflate = 1,
}

impl CompressionMethod {
    pub fn compress<R: Read, W: Write>(&self, mut reader: R, mut writer: W) -> Result<u64> {
        match self {
            CompressionMethod::None => Ok(std::io::copy(&mut reader, &mut writer)? as u64),
            CompressionMethod::Deflate => {
                use zopfli::{Format::Deflate, Options};

                let mut writer = WriterCounter::new(writer);
                zopfli::compress(Options::default(), Deflate, &mut reader, &mut writer)?;

                Ok(writer.total_out)
            }
        }
    }
}

struct WriterCounter<W: Write> {
    inner: W,
    total_out: u64,
}

impl<W: Write> WriterCounter<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            total_out: 0,
        }
    }
}

impl<W: Write> Write for WriterCounter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.total_out += buf.len() as u64;
        self.inner.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl From<CompressionMethod> for u8 {
    fn from(value: CompressionMethod) -> Self {
        match value {
            CompressionMethod::None => 0,
            CompressionMethod::Deflate => 1,
        }
    }
}

impl Encode for CompressionMethod {
    fn encode<W: Write>(&self, mut writer: W) -> crate::Result<usize> {
        u8::from(*self).encode(&mut writer)
    }
}

impl Decode for CompressionMethod {
    fn decode<R: std::io::Read>(mut reader: R) -> crate::Result<Self> {
        let variant = u8::decode(&mut reader)?;

        match variant {
            0 => Ok(CompressionMethod::None),
            1 => Ok(CompressionMethod::Deflate),
            _ => Err(crate::Error::InvalidFileFormat),
        }
    }
}

pub(crate) fn hash_path<P: AsRef<Path>>(path: P) -> u64 {
    let mut hasher = AHasher::default();
    path.as_ref().hash(&mut hasher);
    hasher.finish()
}

pub(crate) const fn _assert_send<T: Send>() {}
pub(crate) const fn _assert_sync<T: Sync>() {}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn encode_decode<T: Encode + Decode>(value: T) -> T {
        let mut bytes = Vec::new();
        value.encode(&mut bytes).unwrap();
        T::decode(&mut bytes.as_slice()).unwrap()
    }

    #[rstest]
    #[case(CompressionMethod::None, 0)]
    #[case(CompressionMethod::Deflate, 42)]
    fn it_encode_decode_header(#[case] method: CompressionMethod, #[case] offset: u64) {
        let header = HpakHeader {
            meta_compression_method: method,
            entries_offset: offset,
        };
        let decoded = encode_decode(header.clone());

        assert_eq!(
            header.meta_compression_method,
            decoded.meta_compression_method
        );
        assert_eq!(header.entries_offset, decoded.entries_offset);
    }

    #[rstest]
    #[case(CompressionMethod::None, 16, 32, 64, 128)]
    #[case(CompressionMethod::Deflate, 32, 64, 128, 256)]
    fn it_encode_decode_file_entry(
        #[case] method: CompressionMethod,
        #[case] hash: u64,
        #[case] meta_offset: u64,
        #[case] meta_size: u64,
        #[case] data_size: u64,
    ) {
        let entry = HpakFileEntry {
            hash,
            compression_method: method,
            meta_offset,
            meta_size,
            data_size,
        };
        let decoded = encode_decode(entry.clone());

        assert_eq!(entry.hash, decoded.hash);
        assert_eq!(entry.compression_method, decoded.compression_method);
        assert_eq!(entry.meta_offset, decoded.meta_offset);
        assert_eq!(entry.meta_size, decoded.meta_size);
        assert_eq!(entry.data_size, decoded.data_size);
    }

    #[rstest]
    #[case(CompressionMethod::None)]
    #[case(CompressionMethod::Deflate)]
    fn it_encode_decode_compression_method(#[case] method: CompressionMethod) {
        assert_eq!(method, encode_decode(method));
    }

    #[test]
    #[should_panic]
    fn if_fails_to_decode_invalid_compression_method() {
        let mut bytes = Vec::new();
        // invalid variant
        u8::MAX.encode(&mut bytes).unwrap();
        // any level
        u8::MIN.encode(&mut bytes).unwrap();

        let _ = CompressionMethod::decode(&mut bytes.as_slice()).unwrap();
    }
}
