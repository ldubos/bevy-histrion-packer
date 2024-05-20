use std::{io::Read, mem::MaybeUninit};

use crate::errors::Error;

pub trait Encoder
where
    Self: Sized,
{
    fn encode(&self) -> Vec<u8>;
    fn decode<R: Read>(reader: &mut R) -> Result<Self, Error>;
}

macro_rules! num_impl {
    ($t:ty, $size:expr) => {
        impl Encoder for $t {
            fn encode(&self) -> Vec<u8> {
                self.to_le_bytes().into()
            }

            fn decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
                let mut bytes = [0u8; $size];
                reader.read_exact(&mut bytes)?;
                Ok(Self::from_le_bytes(bytes))
            }
        }
    };
}

num_impl!(i8, 1);
num_impl!(i16, 2);
num_impl!(i32, 4);
num_impl!(i64, 8);
num_impl!(i128, 16);
num_impl!(u8, 1);
num_impl!(u16, 2);
num_impl!(u32, 4);
num_impl!(u64, 8);
num_impl!(u128, 16);
num_impl!(f32, 4);
num_impl!(f64, 8);

macro_rules! tuple_impl {
    ($($idx:tt $t:tt),+) => {
        impl<$($t,)+> Encoder for ($($t,)+)
        where
            $($t: Encoder,)+
        {
            fn encode(&self) -> Vec<u8> {
                let mut bytes = Vec::new();

                $(bytes.extend_from_slice(&self.$idx.encode());)+

                bytes
            }

            fn decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
                Ok(($($t::decode(reader)?,)+))
            }
        }
    };
}

tuple_impl!(0 T0);
tuple_impl!(0 T0, 1 T1);
tuple_impl!(0 T0, 1 T1, 2 T2);
tuple_impl!(0 T0, 1 T1, 2 T2, 3 T3);
tuple_impl!(0 T0, 1 T1, 2 T2, 3 T3, 4 T4);
tuple_impl!(0 T0, 1 T1, 2 T2, 3 T3, 4 T4, 5 T5);
tuple_impl!(0 T0, 1 T1, 2 T2, 3 T3, 4 T4, 5 T5, 6 T6);
tuple_impl!(0 T0, 1 T1, 2 T2, 3 T3, 4 T4, 5 T5, 6 T6, 7 T7);
tuple_impl!(0 T0, 1 T1, 2 T2, 3 T3, 4 T4, 5 T5, 6 T6, 7 T7, 8 T8);
tuple_impl!(0 T0, 1 T1, 2 T2, 3 T3, 4 T4, 5 T5, 6 T6, 7 T7, 8 T8, 9 T9);
tuple_impl!(0 T0, 1 T1, 2 T2, 3 T3, 4 T4, 5 T5, 6 T6, 7 T7, 8 T8, 9 T9, 10 T10);
tuple_impl!(0 T0, 1 T1, 2 T2, 3 T3, 4 T4, 5 T5, 6 T6, 7 T7, 8 T8, 9 T9, 10 T10, 11 T11);

impl Encoder for String {
    fn encode(&self) -> Vec<u8> {
        let bytes = self.as_bytes().to_owned();
        bytes.encode()
    }

    fn decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        Ok(Self::from_utf8_lossy(&Vec::<u8>::decode(reader)?).into())
    }
}

impl<T, const N: usize> Encoder for [T; N]
where
    T: Encoder,
{
    fn encode(&self) -> Vec<u8> {
        self.iter().flat_map(|v| v.encode()).collect::<Vec<u8>>()
    }

    fn decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let mut arr: [MaybeUninit<T>; N] = unsafe { MaybeUninit::uninit().assume_init() };

        for entry in &mut arr[..] {
            entry.write(T::decode(reader)?);
        }

        Ok(arr.map(|e| unsafe { e.assume_init() }))
    }
}

impl<T> Encoder for Vec<T>
where
    T: Encoder,
{
    fn encode(&self) -> Vec<u8> {
        let mut bytes = (self.len() as u64).encode();
        bytes.extend_from_slice(&self.iter().flat_map(|v| v.encode()).collect::<Vec<u8>>());
        bytes
    }

    fn decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let len = u64::decode(reader)?;
        let mut vec = Vec::<T>::with_capacity(len as usize);

        for idx in 0..len {
            vec[idx as usize] = T::decode(reader)?;
        }

        Ok(vec)
    }
}
