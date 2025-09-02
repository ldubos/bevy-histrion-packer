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

num_impl!(u8: 1, u16: 2, u32: 4, u64: 8);

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
    fn decode<R: Read>(mut reader: R) -> Result<Self> {
        {
            let mut arr: [MaybeUninit<T>; N] = [MaybeUninit::uninit(); N];

            for idx in 0..N {
                match T::decode(&mut reader) {
                    Ok(val) => arr[idx] = MaybeUninit::new(val),
                    Err(err) => {
                        // Drop any values that were already initialized
                        for item in arr.iter_mut().take(idx) {
                            // SAFETY: We have exclusive access to the memory location
                            // and we are within current idx bounds
                            unsafe {
                                item.assume_init_drop();
                            }
                        }

                        return Err(err);
                    }
                }
            }

            Ok(unsafe { *(&arr as *const _ as *const _) })
        }
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
    #[case(String::from("Hello World!"))]
    #[case(PathBuf::from("Hello/World"))]
    #[case(String::from("Hello World!"))]
    #[case([u64::MIN, 0u64, u64::MAX])]
    #[case(vec![u64::MIN, 0u64, u64::MAX, 42u64])]
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
