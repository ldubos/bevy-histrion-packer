use std::{
    io::{BufRead, BufReader, Read},
    path::PathBuf,
};

use super::encoder::Encoder;

#[derive(Debug, Clone)]
pub struct Entry {
    pub(crate) path: PathBuf,
    pub(crate) offset: u64,
    pub(crate) meta_size: u64,
    pub(crate) data_size: u64,
}

impl Encoder for Entry {
    fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();

        out.extend_from_slice(&self.offset.to_le_bytes());
        out.extend_from_slice(&self.meta_size.to_le_bytes());
        out.extend_from_slice(&self.data_size.to_le_bytes());
        out.extend_from_slice(
            self.path
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/")
                .as_bytes(),
        );
        out.push(b'\0');

        out
    }

    fn decode<R: ?Sized>(reader: &mut R) -> Result<Self, Box<dyn std::error::Error>>
    where
        R: Read,
    {
        let mut buffer_u64 = [0; 8];

        // read the offset, meta_size, and data_size.
        reader.read_exact(&mut buffer_u64)?;
        let offset = u64::from_le_bytes(buffer_u64);

        reader.read_exact(&mut buffer_u64)?;
        let meta_size = u64::from_le_bytes(buffer_u64);

        reader.read_exact(&mut buffer_u64)?;
        let data_size = u64::from_le_bytes(buffer_u64);

        let mut path = Vec::new();
        let mut buf_reader = BufReader::new(reader);
        buf_reader.read_until(b'\0', &mut path)?;

        // remove the null byte.
        path.pop();

        Ok(Self {
            offset,
            meta_size,
            data_size,
            path: PathBuf::from(String::from_utf8(path)?),
        })
    }

    fn size_in_bytes(&self) -> u64 {
        8 + 8 + 8 + self.path.to_string_lossy().len() as u64 + 1
    }
}
