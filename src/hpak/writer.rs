use std::{
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

use super::{compression::CompressionAlgorithm, entry::Entry, header::Header};
use crate::{errors::Error, Encode};

/// Configure a [`Writer`] used to generate HPAK archive.
pub struct WriterBuilder<W: Write> {
    pub(super) output: W,
    pub(super) meta_compression: CompressionAlgorithm,
}

impl<W: Write> WriterBuilder<W> {
    pub fn new(output: W) -> Self {
        Self {
            meta_compression: CompressionAlgorithm::Deflate,
            output,
        }
    }

    pub fn meta_compression(mut self, compression: CompressionAlgorithm) -> Self {
        self.meta_compression = compression;
        self
    }

    pub fn build(self) -> Result<Writer<W>, Error> {
        Writer::new(self)
    }
}

pub struct Writer<W: Write> {
    /// The compression method used for entries metadata.
    meta_compression: CompressionAlgorithm,
    output: W,
    temp_data: tempfile::NamedTempFile,
    offset: u64,
    entries_offset: u64,
    entries: Vec<(String, Entry)>,
    can_write: bool,
}

impl<W: Write> Writer<W> {
    pub(super) fn new(config: WriterBuilder<W>) -> Result<Writer<W>, Error> {
        Ok(Writer {
            meta_compression: config.meta_compression,
            output: config.output,
            temp_data: tempfile::NamedTempFile::new()?,
            offset: Header::SIZE as u64,
            entries_offset: 0,
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

        let meta_size = self.meta_compression.compress(meta, &mut self.temp_data)? as u64;
        let data_size = compression_method.compress(data, &mut self.temp_data)? as u64;

        let entry = Entry::new(compression_method, self.offset as u64, meta_size, data_size);
        let entry_path = path.to_string_lossy().to_string();
        let entry_size = entry_path.len() as u64 + Entry::SIZE as u64;

        self.entries.push((entry_path, entry));

        self.offset += meta_size + data_size;
        self.entries_offset += entry_size;

        Ok(())
    }

    /// Finish writing the archive
    pub fn finish(&mut self) -> Result<(), Error> {
        if !self.can_write {
            return Ok(());
        }

        self.can_write = false;
        self.temp_data.flush()?;

        // Write header.
        self.output
            .write_all(&Header::new(self.meta_compression).encode())?;

        // Add offset to entries.
        for (_, entry) in &mut self.entries {
            entry.offset += self.entries_offset;
        }

        // Write the entry table.
        self.output.write_all(&self.entries.encode())?;

        // Write entries data.
        self.temp_data.seek(SeekFrom::Start(0))?;
        std::io::copy(&mut self.temp_data, &mut self.output)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::Encode;

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
        let data_1 = b"The quick brown fox jumps over the lazy dog (data_1)";
        let path_2 = Path::new("a/b/c.data");
        let meta_2 = b"meta_2";
        let data_2 = b"The quick brown fox jumps over the lazy dog (data_2)";

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
        let header = Header::new(CompressionAlgorithm::None);

        let path_1 = path_1.to_string_lossy().to_string();
        let path_2 = path_2.to_string_lossy().to_string();
        let entries_offset = Header::SIZE as u64
            + path_1.len() as u64
            + Entry::SIZE as u64
            + path_2.len() as u64
            + Entry::SIZE as u64;

        ground_truth.extend_from_slice(&header.encode());
        ground_truth.extend_from_slice(
            &vec![
                (
                    path_1,
                    Entry::new(
                        CompressionAlgorithm::None,
                        entries_offset,
                        meta_1.len() as u64,
                        data_1.len() as u64,
                    ),
                ),
                (
                    path_2,
                    Entry::new(
                        CompressionAlgorithm::None,
                        entries_offset + meta_1.len() as u64 + data_1.len() as u64,
                        meta_2.len() as u64,
                        data_2.len() as u64,
                    ),
                ),
            ]
            .encode(),
        );
        ground_truth.extend_from_slice(&meta_1[..]);
        ground_truth.extend_from_slice(&data_1[..]);
        ground_truth.extend_from_slice(&meta_2[..]);
        ground_truth.extend_from_slice(&data_2[..]);

        assert_eq!(&ground_truth, &output);
    }

    #[test]
    fn test_deflate_compression() {
        let mut output = Vec::new();
        let mut writer = WriterBuilder::new(&mut output)
            .meta_compression(CompressionAlgorithm::Deflate)
            .build()
            .unwrap();

        let path_1 = Path::new("ä/b.data");
        let meta_1 = b"meta_1";
        let data_1 = b"The quick brown fox jumps over the lazy dog (data_1)";
        let path_2 = Path::new("a/b/c.data");
        let meta_2 = b"meta_2";
        let data_2 = b"The quick brown fox jumps over the lazy dog (data_2)";
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
        let header = Header::new(CompressionAlgorithm::Deflate);

        let path_1 = path_1.to_string_lossy().to_string();
        let path_2 = path_2.to_string_lossy().to_string();
        let entries_offset = Header::SIZE as u64
            + path_1.len() as u64
            + Entry::SIZE as u64
            + path_2.len() as u64
            + Entry::SIZE as u64;

        ground_truth.extend_from_slice(&header.encode());
        ground_truth.extend_from_slice(
            &vec![
                (
                    path_1,
                    Entry::new(
                        CompressionAlgorithm::Deflate,
                        entries_offset,
                        meta_1_compressed_size,
                        data_1_compressed_size,
                    ),
                ),
                (
                    path_2,
                    Entry::new(
                        CompressionAlgorithm::Deflate,
                        entries_offset + meta_1_compressed_size + data_1_compressed_size,
                        meta_2_compressed_size,
                        data_2_compressed_size,
                    ),
                ),
            ]
            .encode(),
        );
        ground_truth.extend_from_slice(&meta_1_compressed[..]);
        ground_truth.extend_from_slice(&data_1_compressed[..]);
        ground_truth.extend_from_slice(&meta_2_compressed[..]);
        ground_truth.extend_from_slice(&data_2_compressed[..]);

        assert_eq!(&ground_truth, &output);
    }
}
