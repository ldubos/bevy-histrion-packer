use crate::{Error, Result};
use std::{
    io::{Read, Write},
    mem::MaybeUninit,
    path::{Component, PathBuf},
};

pub trait Encode: Sized {
    fn encode<W: Write>(&self, writer: W) -> Result<usize>;
}

pub trait Decode: Sized {
    fn decode<R: Read>(reader: R) -> Result<Self>;
}

macro_rules! num_impl {
    ($t:ty, $size:expr) => {
        impl Encode for $t {
            fn encode<W: Write>(&self, mut writer: W) -> Result<usize> {
                writer.write_all(&self.to_le_bytes())?;
                Ok($size)
            }
        }

        impl Decode for $t {
            fn decode<R: Read>(mut reader: R) -> Result<Self> {
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
            fn encode<W: Write>(&self, mut writer: W) -> Result<usize> {
                let mut written = 0usize;

                $(written += &self.$idx.encode(&mut writer)?;)*

                Ok(written)
            }
        }

        impl<$($t,)*> Decode for ($($t,)*)
        where
            $($t: Decode,)*
        {
            fn decode<R: Read>(mut reader: R) -> Result<Self> {
                Ok(($($t::decode(&mut reader)?,)*))
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
    fn encode<W: Write>(&self, writer: W) -> Result<usize> {
        self.as_bytes().to_owned().encode(writer)
    }
}

impl Decode for String {
    fn decode<R: Read>(reader: R) -> Result<Self> {
        Vec::<u8>::decode(reader)
            .and_then(|bytes| String::from_utf8(bytes).map_err(Error::InvalidUtf8))
    }
}

impl<T, const N: usize> Encode for [T; N]
where
    T: Encode + Copy + 'static,
{
    fn encode<W: Write>(&self, mut writer: W) -> Result<usize> {
        self.iter()
            .try_fold(0usize, |acc, v| Ok(v.encode(&mut writer)? + acc))
    }
}

impl<T, const N: usize> Decode for [T; N]
where
    T: Decode + Copy + 'static,
{
    #[expect(
        clippy::uninit_assumed_init,
        reason = "we use MaybeUninit::assume_init to initialize an array which is later set to the decoded values"
    )]
    fn decode<R: Read>(mut reader: R) -> Result<Self> {
        // FIXME(ldubos): wait for https://github.com/rust-lang/rust/issues/89379 or
        // https://github.com/rust-lang/rust/issues/96097 for better implementation

        // SAFETY: the array should always be initialized before being returned as we iterate over it
        // to decode each entry.
        let mut arr: [T; N] = unsafe { MaybeUninit::uninit().assume_init() };

        arr.iter_mut().try_for_each::<_, Result<()>>(|entry| {
            *entry = T::decode(&mut reader)?;
            Ok(())
        })?;

        Ok(arr)
    }
}

impl<T> Encode for Vec<T>
where
    T: Encode,
{
    fn encode<W: Write>(&self, mut writer: W) -> Result<usize> {
        let written = (self.len() as u64).encode(&mut writer)?;
        self.iter()
            .try_fold(written, move |acc, v| Ok(v.encode(&mut writer)? + acc))
    }
}

impl<T> Decode for Vec<T>
where
    T: Decode,
{
    fn decode<R: Read>(mut reader: R) -> Result<Self> {
        let len = u64::decode(&mut reader)?;
        (0..len).try_fold(Vec::<T>::with_capacity(len as usize), |mut acc, _| {
            acc.push(T::decode(&mut reader)?);
            Ok(acc)
        })
    }
}

impl Encode for PathBuf {
    fn encode<W: Write>(&self, writer: W) -> Result<usize> {
        {
            let mut buf = String::new();

            for component in self.components() {
                match component {
                    Component::RootDir => {}
                    Component::CurDir => buf.push('.'),
                    Component::ParentDir => buf.push_str(".."),
                    Component::Prefix(prefix) => {
                        buf.push_str(&prefix.as_os_str().to_string_lossy());
                        continue;
                    }
                    Component::Normal(s) => buf.push_str(&s.to_string_lossy()),
                }

                buf.push('/');
            }

            #[cfg(windows)]
            {
                use std::os::windows::ffi::OsStrExt as _;

                if self.as_os_str().encode_wide().last() != Some(std::path::MAIN_SEPARATOR as u16)
                    && buf != "/"
                    && buf.ends_with('/')
                {
                    buf.pop();
                }
            }

            buf
        }
        .encode(writer)
    }
}

impl Decode for PathBuf {
    fn decode<R: Read>(reader: R) -> Result<Self> {
        String::decode(reader).map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use super::*;
    use rstest::*;

    #[rstest]
    #[case(u8::MIN)]
    #[case(u8::MAX)]
    #[case(u16::MIN)]
    #[case(u16::MAX)]
    #[case(u32::MIN)]
    #[case(u32::MAX)]
    #[case(u64::MIN)]
    #[case(u64::MAX)]
    #[case(u128::MIN)]
    #[case(u128::MAX)]
    #[case(i8::MIN)]
    #[case(i8::MAX)]
    #[case(i16::MIN)]
    #[case(i16::MAX)]
    #[case(i32::MIN)]
    #[case(i32::MAX)]
    #[case(i64::MIN)]
    #[case(i64::MAX)]
    #[case(i128::MIN)]
    #[case(i128::MAX)]
    #[case(f32::MIN)]
    #[case(f32::MAX)]
    #[case(f64::MIN)]
    #[case(String::from("Hello World!"))]
    #[case(PathBuf::from("Hello/World"))]
    #[case((f64::MIN, 42u128, String::from("Hello World!"), f64::MAX, core::f32::consts::PI))]
    #[case(String::from("Hello World!"))]
    #[case([u128::MIN, 0u128, u128::MAX])]
    #[case(vec![u128::MIN, 0u128, u128::MAX, 42u128])]
    fn it_encode_decode<T: Encode + Decode + PartialEq + Debug>(#[case] value: T) {
        let mut bytes = Vec::new();
        let size = value.encode(&mut bytes).unwrap();
        let decoded = T::decode(&mut bytes.as_slice()).unwrap();

        assert_eq!(size, bytes.len());
        assert_eq!(value, decoded);
    }

    #[test]
    #[cfg(windows)]
    fn it_encode_decode_pathbuf_windows() {
        let path = PathBuf::from(r#"Hello\World\my_file.txt"#);
        let mut bytes = Vec::new();
        let _ = path.encode(&mut bytes).unwrap();
        let decoded = PathBuf::decode(&mut bytes.as_slice()).unwrap();

        assert_eq!(
            String::from("Hello/World/my_file.txt"),
            decoded.to_string_lossy()
        );
    }
}
