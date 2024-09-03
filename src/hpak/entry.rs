use super::compression::CompressionAlgorithm;

#[derive(Debug, Clone)]
pub struct Entry {
    /// The compression method used for the entry's data.
    pub(crate) compression_method: CompressionAlgorithm,
    /// The offset of the entry in the archive.
    pub(crate) offset: u64,
    /// The size of the entry metadata.
    pub(crate) meta_size: u64,
    /// The size of the entry data.
    pub(crate) data_size: u64,
}

impl Entry {
    pub(crate) const SIZE: usize = 1 + 8 + 8 + 8;

    pub fn new(
        compression_method: CompressionAlgorithm,
        offset: u64,
        meta_size: u64,
        data_size: u64,
    ) -> Self {
        Self {
            compression_method,
            offset,
            meta_size,
            data_size,
        }
    }
}

#[cfg(feature = "writer")]
impl crate::Encode for Entry {
    fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::SIZE);

        bytes.extend_from_slice(&self.compression_method.encode());
        bytes.extend_from_slice(&self.offset.encode());
        bytes.extend_from_slice(&self.meta_size.encode());
        bytes.extend_from_slice(&self.data_size.encode());

        bytes
    }
}

impl crate::Decode for Entry {
    fn decode<R: std::io::prelude::Read>(reader: &mut R) -> Result<Self, crate::errors::Error> {
        Ok(Self::new(
            CompressionAlgorithm::decode(reader)?,
            u64::decode(reader)?,
            u64::decode(reader)?,
            u64::decode(reader)?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Decode, Encode};

    #[test]
    fn test_entry_encode_decode() {
        let entry = Entry::new(CompressionAlgorithm::Deflate, 42, 16, 32);
        let bytes = entry.encode();
        let decoded = Entry::decode(&mut bytes.as_slice()).unwrap();

        assert_eq!(entry.compression_method, decoded.compression_method);
        assert_eq!(entry.offset, decoded.offset);
        assert_eq!(entry.meta_size, decoded.meta_size);
        assert_eq!(entry.data_size, decoded.data_size);
    }
}
