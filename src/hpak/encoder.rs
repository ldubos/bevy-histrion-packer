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

        for _ in 0..len {
            vec.push(T::decode(reader)?);
        }

        Ok(vec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! test_num_impl {
        ($f:ident, $t:ty) => {
            #[test]
            fn $f() {
                let bytes = <$t>::MIN.encode();
                let decoded = <$t>::decode(&mut bytes.as_slice()).unwrap();

                assert_eq!(<$t>::MIN, decoded);

                let bytes = <$t>::MAX.encode();
                let decoded = <$t>::decode(&mut bytes.as_slice()).unwrap();

                assert_eq!(<$t>::MAX, decoded);
            }
        };
    }

    test_num_impl!(test_i8_encode_decode, i8);
    test_num_impl!(test_i16_encode_decode, i16);
    test_num_impl!(test_i32_encode_decode, i32);
    test_num_impl!(test_i64_encode_decode, i64);
    test_num_impl!(test_i128_encode_decode, i128);
    test_num_impl!(test_u8_encode_decode, u8);
    test_num_impl!(test_u16_encode_decode, u16);
    test_num_impl!(test_u32_encode_decode, u32);
    test_num_impl!(test_u64_encode_decode, u64);
    test_num_impl!(test_u128_encode_decode, u128);
    test_num_impl!(test_f32_encode_decode, f32);
    test_num_impl!(test_f64_encode_decode, f64);

    #[test]
    fn test_string_encode_decode() {
        let string = "Hello World!".to_string();
        let bytes = string.encode();
        let decoded = String::decode(&mut bytes.as_slice()).unwrap();

        assert_eq!(string, decoded);
    }

    #[test]
    fn test_array_encode_decode() {
        let array = [u16::MIN, u16::MAX];
        let bytes = array.encode();
        let decoded = <[u16; 2]>::decode(&mut bytes.as_slice()).unwrap();

        assert_eq!(array, decoded);
    }

    #[test]
    fn test_vec_encode_decode() {
        let vec = vec![u16::MIN, u16::MAX];
        let bytes = vec.encode();
        let decoded = Vec::<u16>::decode(&mut bytes.as_slice()).unwrap();

        assert_eq!(vec, decoded);
    }

    #[test]
    fn test_tuple_encode_decode() {
        let tuple = (0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11);
        let bytes = tuple.encode();
        let decoded = <(u16, u16, u16, u16, u16, u16, u16, u16, u16, u16, u16, u16)>::decode(
            &mut bytes.as_slice(),
        )
        .unwrap();

        assert_eq!(tuple, decoded);
    }
}
