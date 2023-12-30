use std::io::Read;

use crate::errors::HPakError;

use super::{encoder::Encoder, entry::Entry};

#[derive(Default, Debug, Clone)]
pub struct Header {
    pub(crate) entries: Vec<Entry>,
}

impl Encoder for Header {
    fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();

        out.extend_from_slice(&(self.entries.len() as u64).to_le_bytes());

        for entry in &self.entries {
            out.extend_from_slice(&entry.encode());
        }

        out
    }

    fn decode<R: ?Sized>(reader: &mut R) -> Result<Self, HPakError>
    where
        R: Read,
    {
        // convert the length of the entries vector to a u64 and add it to the output buffer.
        let mut buffer_u64 = [0; 8];
        reader.read_exact(&mut buffer_u64)?;
        let num_entries = u64::from_le_bytes(buffer_u64);

        let mut entries = Vec::with_capacity(num_entries as usize);

        for _ in 0..num_entries {
            let entry = Entry::decode(reader)?;
            entries.push(entry);
        }

        Ok(Self { entries })
    }

    fn size_in_bytes(&self) -> u64 {
        let mut size = 8;

        for entry in &self.entries {
            size += entry.size_in_bytes();
        }

        size
    }
}
