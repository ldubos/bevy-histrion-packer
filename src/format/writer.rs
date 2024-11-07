use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use super::*;
use crate::{encoding::*, Error, Result};

pub struct HpakWriter {
    output: File,
    meta_compression_method: CompressionMethod,
    entries: BTreeMap<PathBuf, HpakFileEntry>,
    with_padding: bool,
    finalized: bool,
}

impl HpakWriter {
    /// Create a new HPAK writer.
    /// The `meta_compression_method` is used to compress the metadata of the assets.
    /// If `with_padding` is true, padding will be added to align entries to 4096 bytes.
    pub fn new(
        path: impl AsRef<Path>,
        meta_compression_method: CompressionMethod,
        with_padding: bool,
    ) -> Result<Self> {
        let mut output = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;

        // write dummy header, overwritten in finalize()
        let header = HpakHeader {
            meta_compression_method,
            entries_offset: 0,
        };
        header.encode(&mut output)?;

        Ok(Self {
            output,
            meta_compression_method,
            entries: BTreeMap::new(),
            with_padding,
            finalized: false,
        })
    }

    /// Add an entry to the archive.
    pub fn add_entry(
        &mut self,
        path: impl AsRef<Path>,
        meta: impl Read,
        data: impl Read,
        compression_method: CompressionMethod,
    ) -> Result<()> {
        if self.finalized {
            return Err(Error::CannotAddEntryAfterFinalize);
        }

        let path = path.as_ref();

        if self.entries.contains_key(path) {
            return Err(Error::DuplicateEntry(path.to_path_buf()));
        }

        self.pad_to_alignment()?;

        let meta_offset = self.offset()?;

        let meta_size = self
            .meta_compression_method
            .compress(meta, &mut self.output)?;
        let data_size = compression_method.compress(data, &mut self.output)?;

        let entry = HpakFileEntry {
            hash: hash_path(path),
            compression_method,
            meta_offset,
            meta_size,
            data_size,
        };

        self.entries.insert(path.to_path_buf(), entry);

        Ok(())
    }

    /// Write the entries table and the final header then flush the writer.
    pub fn finalize(&mut self) -> Result<()> {
        if self.finalized {
            return Ok(());
        }

        self.finalized = true;

        self.pad_to_alignment()?;

        let header = HpakHeader {
            meta_compression_method: self.meta_compression_method,
            entries_offset: self.offset()?,
        };

        let mut entries = HpakEntries {
            directories: HashTable::new(),
            files: HashTable::new(),
        };

        // build directory/files tables
        for (path, entry) in self.entries.iter() {
            let mut ancestors = path.ancestors();
            let mut prev = ancestors.next().unwrap().to_path_buf();

            // for each ancestor directory, create or update the directory entry
            for ancestor in ancestors {
                let ancestor_hash = hash_path(ancestor);
                let ancestor: PathBuf = ancestor.into();

                let entry = entries
                    .directories
                    .entry(
                        ancestor_hash,
                        |directory| directory.hash == ancestor_hash,
                        HpakDirectoryEntry::hash,
                    )
                    .or_insert(HpakDirectoryEntry {
                        hash: ancestor_hash,
                        entries: Vec::new(),
                    })
                    .into_mut();

                // add the child entry to the directory
                if entry.entries.iter().all(|path| *path != prev) {
                    entry.entries.push(prev);
                }

                prev = ancestor;
            }

            // add the file entry to the file table
            let path = hash_path(path.as_path());
            entries
                .files
                .insert_unique(path, entry.clone(), HpakFileEntry::hash);
        }

        entries.encode(&mut self.output)?;

        self.output.flush()?;

        // return to the beginning of the file and overwrite dummy header
        self.output.seek(SeekFrom::Start(0))?;
        header.encode(&mut self.output)?;

        self.output.flush()?;

        Ok(())
    }

    #[inline]
    fn offset(&mut self) -> Result<u64> {
        Ok(self.output.stream_position()?)
    }

    fn pad_to_alignment(&mut self) -> Result<()> {
        if !self.with_padding {
            return Ok(());
        }

        const ALIGNMENT: u64 = 4096;

        let offset = self.offset()?;

        let aligned = (offset + (ALIGNMENT - 1)) & !(ALIGNMENT - 1);
        let padding = aligned - offset;

        if padding > 0 {
            let padding_bytes = vec![0u8; padding as usize];
            self.output.write_all(&padding_bytes)?;
        }

        self.output.flush()?;

        Ok(())
    }
}
