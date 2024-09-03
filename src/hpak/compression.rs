use std::io::{Read, Write};

use crate::errors::Error;

#[repr(u8)]
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionAlgorithm {
    /// No compression.
    #[default]
    None = 0,
    /// Uses deflate from [flate2](https://crates.io/crates/flate2), fast decompression speed for an average compression ratio.
    Deflate = 1,
}

impl CompressionAlgorithm {
    pub fn compress<R, W>(&self, reader: &mut R, writer: &mut W) -> Result<usize, Error>
    where
        R: Read,
        W: Write,
    {
        Ok(match self {
            CompressionAlgorithm::None => std::io::copy(reader, writer)? as usize,
            CompressionAlgorithm::Deflate => {
                let mut compressor =
                    flate2::read::DeflateEncoder::new(reader, flate2::Compression::new(9));
                std::io::copy(&mut compressor, writer)? as usize
            }
        })
    }
}

impl crate::Encode for CompressionAlgorithm {
    fn encode(&self) -> Vec<u8> {
        vec![*self as u8]
    }
}

impl crate::Decode for CompressionAlgorithm {
    fn decode<R: std::io::prelude::Read>(reader: &mut R) -> Result<Self, Error> {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf)?;
        let value = buf[0];
        match value {
            0 => Ok(CompressionAlgorithm::None),
            1 => Ok(CompressionAlgorithm::Deflate),
            _ => Err(Error::InvalidFileFormat),
        }
    }
}
