use std::{
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

use super::{compression::CompressionAlgorithm, encoder::Encoder, entry::Entry, header::Header};
use crate::errors::Error;

/// Configure a [`Writer`] used to generate HPAK archive.
pub struct WriterBuilder<W: Write> {
    pub(super) output: W,
    pub(super) meta_compression: CompressionAlgorithm,
}

impl<W: Write> WriterBuilder<W> {
    pub fn new(output: W) -> Self {
        Self {
            #[cfg(feature = "brotli")]
            meta_compression: CompressionAlgorithm::Brotli,
            #[cfg(not(feature = "brotli"))]
            meta_compression: DEFAULT_FALLBACK_COMPRESSION_METHOD,
            output,
        }
    }

    pub fn meta_compression(mut self, compression: CompressionAlgorithm) -> Self {
        self.meta_compression = compression;
        self
    }

    pub fn build(self) -> Result<Writer<W>, Error> {
        Writer::init(self)
    }
}

#[cfg(feature = "deflate")]
pub const DEFAULT_FALLBACK_COMPRESSION_METHOD: CompressionAlgorithm = CompressionAlgorithm::Deflate;
#[cfg(not(feature = "deflate"))]
pub const DEFAULT_FALLBACK_COMPRESSION_METHOD: CompressionAlgorithm = CompressionAlgorithm::None;

pub struct Writer<W: Write> {
    /// The compression method used for entries metadata.
    meta_compression: CompressionAlgorithm,
    output: W,
    temp: tempfile::NamedTempFile,
    offset: u64,
    entries: Vec<(String, Entry)>,
    can_write: bool,
}

impl<W: Write> Writer<W> {
    pub(super) fn init(config: WriterBuilder<W>) -> Result<Writer<W>, Error> {
        Ok(Writer {
            meta_compression: config.meta_compression,
            output: config.output,
            temp: tempfile::NamedTempFile::new()?,
            offset: Header::SIZE,
            entries: Vec::new(),
            can_write: true,
        })
    }

    /// Add an entry to the archive.
    pub fn add_entry<M, D>(
        &mut self,
        path: &Path,
        meta: &mut M,
        data: &mut D,
        compression_method: CompressionAlgorithm,
    ) -> Result<(), Error>
    where
        M: Read,
        D: Read,
    {
        if !self.can_write {
            return Err(Error::CannotAddEntryAfterFinalize);
        }

        let meta_size = self.meta_compression.compress(meta, &mut self.temp)? as u64;
        let data_size = compression_method.compress(data, &mut self.temp)? as u64;

        let entry = Entry::new(compression_method, self.offset, meta_size, data_size);
        self.entries
            .push((path.to_string_lossy().to_string(), entry));

        self.offset += meta_size + data_size;

        Ok(())
    }

    /// Finish writing the archive
    pub fn finish(&mut self) -> Result<(), Error> {
        if !self.can_write {
            return Ok(());
        }

        self.can_write = false;
        self.temp.flush()?;

        self.write_header()?;

        self.temp.seek(SeekFrom::Start(0))?;

        // Write entries data.
        std::io::copy(&mut self.temp, &mut self.output)?;

        // Write the entry table.
        self.output.write_all(&self.entries.encode())?;

        Ok(())
    }

    fn write_header(&mut self) -> Result<(), Error> {
        let bytes = Header::new(self.meta_compression, self.offset).encode();
        self.output.write_all(&bytes)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_compression() {
        let mut output = Vec::new();
        let mut writer = WriterBuilder::new(&mut output)
            .meta_compression(CompressionAlgorithm::None)
            .build()
            .unwrap();

        let path_1 = Path::new("ä/b.data");
        let meta_1 = b"meta_1";
        let data_1 = b"data_1";
        let path_2 = Path::new("a/b/c.data");
        let meta_2 = b"meta_2";
        let data_2 = b"data_2";

        writer
            .add_entry(
                path_1,
                &mut meta_1.as_slice(),
                &mut data_1.as_slice(),
                CompressionAlgorithm::None,
            )
            .unwrap();
        writer
            .add_entry(
                path_2,
                &mut meta_2.as_slice(),
                &mut data_2.as_slice(),
                CompressionAlgorithm::None,
            )
            .unwrap();
        writer.finish().unwrap();

        let mut ground_truth = Vec::new();
        let header = Header::new(
            CompressionAlgorithm::None,
            Header::SIZE + (meta_1.len() + data_1.len() + meta_2.len() + data_2.len()) as u64,
        );

        ground_truth.extend_from_slice(&header.encode());
        ground_truth.extend_from_slice(&meta_1[..]);
        ground_truth.extend_from_slice(&data_1[..]);
        ground_truth.extend_from_slice(&meta_2[..]);
        ground_truth.extend_from_slice(&data_2[..]);
        ground_truth.extend_from_slice(
            &vec![
                (
                    path_1.to_string_lossy().to_string(),
                    Entry::new(
                        CompressionAlgorithm::None,
                        Header::SIZE,
                        meta_1.len() as u64,
                        data_1.len() as u64,
                    ),
                ),
                (
                    path_2.to_string_lossy().to_string(),
                    Entry::new(
                        CompressionAlgorithm::None,
                        Header::SIZE + meta_1.len() as u64 + data_1.len() as u64,
                        meta_2.len() as u64,
                        data_2.len() as u64,
                    ),
                ),
            ]
            .encode(),
        );

        assert_eq!(&ground_truth, &output);
    }

    #[test]
    #[cfg(feature = "deflate")]
    fn test_deflate_compression() {
        let mut output = Vec::new();
        let mut writer = WriterBuilder::new(&mut output)
            .meta_compression(CompressionAlgorithm::Deflate)
            .build()
            .unwrap();

        let path_1 = Path::new("ä/b.data");
        let meta_1 = b"meta_1";
        let data_1 = b"data_1";
        let path_2 = Path::new("a/b/c.data");
        let meta_2 = b"meta_2";
        let data_2 = b"data_2";
        let mut meta_1_compressed = Vec::new();
        let mut data_1_compressed = Vec::new();
        let mut meta_2_compressed = Vec::new();
        let mut data_2_compressed = Vec::new();

        let meta_1_compressed_size = CompressionAlgorithm::Deflate
            .compress(&mut meta_1.as_slice(), &mut meta_1_compressed)
            .unwrap() as u64;
        let data_1_compressed_size = CompressionAlgorithm::Deflate
            .compress(&mut data_1.as_slice(), &mut data_1_compressed)
            .unwrap() as u64;
        let meta_2_compressed_size = CompressionAlgorithm::Deflate
            .compress(&mut meta_2.as_slice(), &mut meta_2_compressed)
            .unwrap() as u64;
        let data_2_compressed_size = CompressionAlgorithm::Deflate
            .compress(&mut data_2.as_slice(), &mut data_2_compressed)
            .unwrap() as u64;

        writer
            .add_entry(
                path_1,
                &mut meta_1.as_slice(),
                &mut data_1.as_slice(),
                CompressionAlgorithm::Deflate,
            )
            .unwrap();
        writer
            .add_entry(
                path_2,
                &mut meta_2.as_slice(),
                &mut data_2.as_slice(),
                CompressionAlgorithm::Deflate,
            )
            .unwrap();
        writer.finish().unwrap();

        let mut ground_truth = Vec::new();
        let header = Header::new(
            CompressionAlgorithm::Deflate,
            Header::SIZE
                + (meta_1_compressed.len()
                    + data_1_compressed.len()
                    + meta_2_compressed.len()
                    + data_2_compressed.len()) as u64,
        );

        ground_truth.extend_from_slice(&header.encode());
        ground_truth.extend_from_slice(&meta_1_compressed[..]);
        ground_truth.extend_from_slice(&data_1_compressed[..]);
        ground_truth.extend_from_slice(&meta_2_compressed[..]);
        ground_truth.extend_from_slice(&data_2_compressed[..]);
        ground_truth.extend_from_slice(
            &vec![
                (
                    path_1.to_string_lossy().to_string(),
                    Entry::new(
                        CompressionAlgorithm::Deflate,
                        Header::SIZE,
                        meta_1_compressed_size,
                        data_1_compressed_size,
                    ),
                ),
                (
                    path_2.to_string_lossy().to_string(),
                    Entry::new(
                        CompressionAlgorithm::Deflate,
                        Header::SIZE + meta_1_compressed_size + data_1_compressed_size,
                        meta_2_compressed_size,
                        data_2_compressed_size,
                    ),
                ),
            ]
            .encode(),
        );

        assert_eq!(&ground_truth, &output);
    }
}
