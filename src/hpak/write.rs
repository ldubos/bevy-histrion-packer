use std::{
    io::{Read, Seek, Write},
    path::Path,
};

use super::{encoder::Encoder, entry::Entry, header::Header};

pub struct HPakWriter<W: Write> {
    header: Header,
    output: W,
    temp: tempfile::SpooledTempFile,
    offset: u64,
    can_write: bool,
}

impl<W> HPakWriter<W>
where
    W: Write,
{
    pub fn new(writer: W) -> Self {
        Self {
            header: Header::default(),
            output: writer,
            temp: tempfile::SpooledTempFile::new(1024 * 1024),
            offset: 0,
            can_write: true,
        }
    }

    pub fn add_entry<M: ?Sized, D: ?Sized>(
        &mut self,
        path: &Path,
        meta: &mut M,
        data: &mut D,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        M: Read,
        D: Read,
    {
        if !self.can_write {
            return Err("cannot add entry after finalize call".into());
        }

        let meta_size = {
            let mut encoder = flate2::read::ZlibEncoder::new(meta, flate2::Compression::best());
            std::io::copy(&mut encoder, &mut self.temp)?;
            self.temp.flush()?;
            encoder.total_out()
        };

        let data_size = {
            let mut encoder = flate2::read::ZlibEncoder::new(data, flate2::Compression::best());
            std::io::copy(&mut encoder, &mut self.temp)?;
            self.temp.flush()?;
            encoder.total_out()
        };

        self.header.entries.push(Entry {
            path: path.to_path_buf(),
            offset: self.offset,
            meta_size,
            data_size,
        });

        self.offset += meta_size + data_size;

        Ok(())
    }

    pub fn finalize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if !self.can_write {
            return Err("cannot finalize twice".into());
        }

        let header_size = self.header.size_in_bytes();

        for entry in &mut self.header.entries {
            entry.offset += header_size;
        }

        let header = self.header.encode();
        self.output.write_all(&header)?;

        self.temp.seek(std::io::SeekFrom::Start(0))?;
        std::io::copy(&mut self.temp, &mut self.output)?;

        self.output.flush()?;
        self.can_write = false;

        Ok(())
    }
}
