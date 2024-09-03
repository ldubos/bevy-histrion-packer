use std::{io::Read, mem::MaybeUninit};

use crate::errors::Error;

pub trait Encode
where
    Self: Sized,
{
    fn encode(&self) -> Vec<u8>;
}

pub trait Decode
where
    Self: Sized,
{
    fn decode<R: Read>(reader: &mut R) -> Result<Self, Error>;
}

macro_rules! num_impl {
    ($t:ty, $size:expr) => {
        impl Encode for $t {
            fn encode(&self) -> Vec<u8> {
                self.to_le_bytes().into()
            }
        }

        impl Decode for $t {
            fn decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
                let mut bytes = [0u8; $size];
                reader
                    .read_exact(&mut bytes)
                    .map(|_| Self::from_le_bytes(bytes))
                    .map_err(|e| Error::Io(e.into()))
            }
        }
    };
    ($($t:ty: $size:expr),*) => {
        $(
            num_impl!($t, $size);
        )*
    };
}

num_impl!(i8: 1, i16: 2, i32: 4, i64: 8, i128: 16, u8: 1, u16: 2, u32: 4, u64: 8, u128: 16, f32: 4, f64: 8);

macro_rules! tuple_impl {
    ($($idx:tt $t:tt),*) => {
        impl<$($t,)*> Encode for ($($t,)*)
        where
            $($t: Encode,)*
        {
            fn encode(&self) -> Vec<u8> {
                let mut bytes = Vec::new();

                $(bytes.extend_from_slice(&self.$idx.encode());)*

                bytes
            }
        }

        impl<$($t,)*> Decode for ($($t,)*)
        where
            $($t: Decode,)*
        {
            fn decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
                Ok(($($t::decode(reader)?,)*))
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
tuple_impl!(0 T0, 1 T1, 2 T2, 3 T3, 4 T4, 5 T5, 6 T6, 7 T7, 8 T8, 9 T9, 10 T10, 11 T11, 12 T12);
tuple_impl!(0 T0, 1 T1, 2 T2, 3 T3, 4 T4, 5 T5, 6 T6, 7 T7, 8 T8, 9 T9, 10 T10, 11 T11, 12 T12, 13 T13);
tuple_impl!(0 T0, 1 T1, 2 T2, 3 T3, 4 T4, 5 T5, 6 T6, 7 T7, 8 T8, 9 T9, 10 T10, 11 T11, 12 T12, 13 T13, 14 T14);
tuple_impl!(0 T0, 1 T1, 2 T2, 3 T3, 4 T4, 5 T5, 6 T6, 7 T7, 8 T8, 9 T9, 10 T10, 11 T11, 12 T12, 13 T13, 14 T14, 15 T15);

impl Encode for String {
    fn encode(&self) -> Vec<u8> {
        self.as_bytes().to_owned().encode()
    }
}

impl Decode for String {
    fn decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        Vec::<u8>::decode(reader)
            .and_then(|bytes| String::from_utf8(bytes).map_err(|e| Error::InvalidUtf8(e.into())))
    }
}

impl<T, const N: usize> Encode for [T; N]
where
    T: Encode,
{
    fn encode(&self) -> Vec<u8> {
        self.iter().flat_map(|v| v.encode()).collect::<Vec<u8>>()
    }
}

impl<T, const N: usize> Decode for [T; N]
where
    T: Decode,
{
    fn decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let mut arr: [T; N] = unsafe { MaybeUninit::uninit().assume_init() };

        arr.iter_mut()
            .try_for_each::<_, Result<(), Error>>(|entry| {
                *entry = T::decode(reader)?;

                Ok(())
            })?;

        Ok(arr)
    }
}

impl<T> Encode for Vec<T>
where
    T: Encode,
{
    fn encode(&self) -> Vec<u8> {
        let mut bytes = (self.len() as u64).encode();
        bytes.extend_from_slice(&self.iter().flat_map(|v| v.encode()).collect::<Vec<u8>>());
        bytes
    }
}

impl<T> Decode for Vec<T>
where
    T: Decode,
{
    fn decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let len = u64::decode(reader)?;
        let mut vec = Vec::<T>::with_capacity(len as usize);

        for _ in 0..len {
            vec.push(T::decode(reader)?);
        }

        Ok(vec)
    }
}
