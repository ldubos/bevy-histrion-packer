use crate::errors::Error;

use super::compression::CompressionAlgorithm;
use super::encoder::Encoder;

#[derive(Debug, Clone, Copy)]
pub struct Header {
    /// HPAK
    pub(crate) magic: [u8; crate::MAGIC_LEN],
    /// Version of the file format
    pub(crate) version: u16,
    /// The compression method for entries' metadata
    pub(crate) metadata_compression_method: CompressionAlgorithm,
    /// The position of the entry table
    pub(crate) entry_table_offset: u64,
}

impl Header {
    pub const SIZE: u64 = crate::MAGIC_LEN as u64 + 2 + 1 + 8;

    #[allow(dead_code)]
    pub fn new(metadata_compression_method: CompressionAlgorithm, entry_table_offset: u64) -> Self {
        Self {
            magic: *crate::MAGIC,
            version: crate::VERSION,
            metadata_compression_method,
            entry_table_offset,
        }
    }
}

impl Encoder for Header {
    fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::SIZE as usize);

        bytes.extend_from_slice(&self.magic.encode());
        bytes.extend_from_slice(&self.version.encode());
        bytes.extend_from_slice(&self.metadata_compression_method.encode());
        bytes.extend_from_slice(&self.entry_table_offset.encode());

        bytes
    }

    fn decode<R: std::io::prelude::Read>(reader: &mut R) -> Result<Self, Error> {
        let magic = <[u8; crate::MAGIC_LEN]>::decode(reader)?;

        if !magic.iter().zip(crate::MAGIC.iter()).all(|(a, b)| a == b) {
            return Err(Error::InvalidFileFormat);
        }

        let version = u16::decode(reader)?;

        if version != crate::VERSION {
            return Err(Error::BadVersion(version));
        }

        Ok(Self {
            magic,
            version,
            metadata_compression_method: CompressionAlgorithm::decode(reader)?,
            entry_table_offset: u64::decode(reader)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_encode_decode() {
        let header = Header::new(CompressionAlgorithm::Deflate, 42);
        let bytes = header.encode();
        let decoded = Header::decode(&mut bytes.as_slice()).unwrap();

        assert_eq!(header.magic, decoded.magic);
        assert_eq!(header.version, decoded.version);
        assert_eq!(
            header.metadata_compression_method,
            decoded.metadata_compression_method
        );
        assert_eq!(header.entry_table_offset, decoded.entry_table_offset);
    }
}
