use std::{io::Read, io::Write};

use flate2::Compression;

use crate::errors::Error;

use super::encoder::Encoder;

#[repr(u8)]
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionAlgorithm {
    /// No compression.
    #[default]
    None = 0,
    /// Uses deflate from [Falte2](https://crates.io/crates/flate2), fast decompression speed for an average compression ratio.
    #[cfg(feature = "deflate")]
    Deflate = 1,
    /// Uses gzip from [Flate2](https://crates.io/crates/flate2)
    #[cfg(feature = "gzip")]
    Gzip = 2,
    /// Uses zlib from [Flate2](https://crates.io/crates/flate2)
    #[cfg(feature = "zlib")]
    Zlib = 3,
    /// Uses [Brotli](https://crates.io/crates/brotli), high compression ratio but slower decompression speed.
    /// Works best for text like json, toml, ron, etc.
    #[cfg(feature = "brotli")]
    Brotli = 4,
}

impl CompressionAlgorithm {
    pub fn compress<R, W>(&self, reader: &mut R, writer: &mut W) -> Result<usize, Error>
    where
        R: Read,
        W: Write,
    {
        Ok(match self {
            CompressionAlgorithm::None => std::io::copy(reader, writer)? as usize,
            #[cfg(feature = "deflate")]
            CompressionAlgorithm::Deflate => {
                let mut compressor = flate2::read::DeflateEncoder::new(reader, Compression::new(5));
                std::io::copy(&mut compressor, writer)? as usize
            }
            #[cfg(feature = "gzip")]
            CompressionAlgorithm::Gzip => {
                let mut compressor = flate2::read::GzEncoder::new(reader, Compression::new(5));
                std::io::copy(&mut compressor, writer)? as usize
            }
            #[cfg(feature = "zlib")]
            CompressionAlgorithm::Zlib => {
                let mut compressor = flate2::read::ZlibEncoder::new(reader, Compression::new(5));
                std::io::copy(&mut compressor, writer)? as usize
            }
            #[cfg(feature = "brotli")]
            CompressionAlgorithm::Brotli => {
                let mut compressor = brotli::CompressorReader::new(reader, 4096, 11, 21);
                std::io::copy(&mut compressor, writer)? as usize
            }
        })
    }
}

impl Encoder for CompressionAlgorithm {
    fn encode(&self) -> Vec<u8> {
        vec![*self as u8]
    }

    fn decode<R: std::io::prelude::Read>(reader: &mut R) -> Result<Self, Error> {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf)?;
        let value = buf[0];
        match value {
            0 => Ok(CompressionAlgorithm::None),
            #[cfg(feature = "deflate")]
            1 => Ok(CompressionAlgorithm::Deflate),
            #[cfg(feature = "gzip")]
            2 => Ok(CompressionAlgorithm::Gzip),
            #[cfg(feature = "zlib")]
            3 => Ok(CompressionAlgorithm::Zlib),
            #[cfg(feature = "brotli")]
            4 => Ok(CompressionAlgorithm::Brotli),
            _ => Err(Error::InvalidFileFormat),
        }
    }
}
