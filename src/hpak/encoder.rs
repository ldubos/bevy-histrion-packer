use std::{error::Error, io::Read};

pub trait Encoder
where
    Self: Sized,
{
    fn encode(&self) -> Vec<u8>;
    fn decode<R: ?Sized>(reader: &mut R) -> Result<Self, Box<dyn Error>>
    where
        R: Read;
    fn size_in_bytes(&self) -> u64;
}
