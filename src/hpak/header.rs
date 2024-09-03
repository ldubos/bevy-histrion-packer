use crate::errors::Error;

use super::compression::CompressionAlgorithm;

#[derive(Debug, Clone, Copy)]
pub struct Header {
    /// HPAK
    pub(crate) magic: [u8; crate::MAGIC_LEN],
    /// Version of the file format
    pub(crate) version: u16,
    /// The compression method for entries' metadata
    pub(crate) metadata_compression_method: CompressionAlgorithm,
}

impl Header {
    pub(crate) const SIZE: usize = crate::MAGIC_LEN + 2 + 1;

    pub fn new(metadata_compression_method: CompressionAlgorithm) -> Self {
        Self {
            magic: *crate::MAGIC,
            version: crate::VERSION,
            metadata_compression_method,
        }
    }
}

impl crate::Encode for Header {
    fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::SIZE);

        bytes.extend_from_slice(&self.magic.encode());
        bytes.extend_from_slice(&self.version.encode());
        bytes.extend_from_slice(&self.metadata_compression_method.encode());

        bytes
    }
}

impl crate::Decode for Header {
    fn decode<R: std::io::prelude::Read>(reader: &mut R) -> Result<Self, Error> {
        let magic = <[u8; crate::MAGIC_LEN]>::decode(reader)?;

        if !magic.iter().zip(crate::MAGIC.iter()).all(|(a, b)| a == b) {
            return Err(Error::InvalidFileFormat);
        }

        let version = u16::decode(reader)?;

        if version != crate::VERSION {
            return Err(Error::BadVersion(version));
        }

        Ok(Self::new(CompressionAlgorithm::decode(reader)?))
    }
}

#[cfg(test)]
mod tests {
    use crate::{Decode, Encode, MAGIC, MAGIC_LEN, VERSION};

    use super::*;

    #[test]
    fn test_header_encode_decode() {
        let header = Header::new(CompressionAlgorithm::Deflate);
        let bytes = header.encode();
        let decoded = Header::decode(&mut bytes.as_slice()).unwrap();

        assert_eq!(header.magic, decoded.magic);
        assert_eq!(header.version, decoded.version);
        assert_eq!(
            header.metadata_compression_method,
            decoded.metadata_compression_method
        );
    }

    #[test]
    #[should_panic]
    fn test_header_decode_bad_magic() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0u8; MAGIC_LEN]);
        bytes.extend_from_slice(&[0u8; 2]);
        bytes.extend_from_slice(&[1u8]);

        Header::decode(&mut bytes.as_slice()).unwrap();
    }

    #[test]
    #[should_panic]
    fn test_header_decode_bad_version() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&(VERSION - 1).encode());

        Header::decode(&mut bytes.as_slice()).unwrap();
    }
}
