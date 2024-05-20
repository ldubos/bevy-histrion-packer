use std::{
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use super::{compression::CompressionAlgorithm, encoder::Encoder, entry::Entry, header::Header};
use crate::{
    errors::Error,
    utils::{get_meta_loader_settings, get_meta_loader_type_path},
};

pub type CompressionMethodFn = dyn Fn(&Path, &[u8]) -> CompressionAlgorithm;

/// Configure a [`Writer`] used to generate HPAK archive.
pub struct WriterBuilder<W: Write> {
    /// The compression method used for entries metadata.
    pub(super) meta_compression: CompressionAlgorithm,
    /// The callback used to determine the compression method used for entries data.
    /// Takes the file path and metadata bytes to returns the compression method.
    pub(super) data_compression_fn: &'static CompressionMethodFn,
    pub(super) output: W,
}

impl<W: Write> WriterBuilder<W> {
    pub fn new(output: W) -> Self {
        Self {
            #[cfg(feature = "brotli")]
            meta_compression: CompressionAlgorithm::Brotli,
            #[cfg(not(feature = "brotli"))]
            meta_compression: DEFAULT_FALLBACK_COMPRESSION_METHOD,
            data_compression_fn: &default_data_compression_fn,
            output,
        }
    }

    pub fn meta_compression(mut self, compression: CompressionAlgorithm) -> Self {
        self.meta_compression = compression;
        self
    }

    pub fn data_compression_fn(
        mut self,
        data_compression_fn: &'static CompressionMethodFn,
    ) -> Self {
        self.data_compression_fn = data_compression_fn;
        self
    }

    pub fn build(self) -> Result<Writer<W>, Error> {
        Writer::init(self)
    }
}

#[cfg(feature = "deflate")]
const DEFAULT_FALLBACK_COMPRESSION_METHOD: CompressionAlgorithm = CompressionAlgorithm::Deflate;
#[cfg(all(not(feature = "deflate"), feature = "gzip"))]
const DEFAULT_FALLBACK_COMPRESSION_METHOD: CompressionAlgorithm = CompressionAlgorithm::Gzip;
#[cfg(all(not(feature = "deflate"), not(feature = "gzip"), feature = "zlib"))]
const DEFAULT_FALLBACK_COMPRESSION_METHOD: CompressionAlgorithm = CompressionAlgorithm::Zlib;
#[cfg(all(not(feature = "deflate"), not(feature = "gzip"), not(feature = "zlib"),))]
const DEFAULT_FALLBACK_COMPRESSION_METHOD: CompressionAlgorithm = CompressionAlgorithm::None;

fn default_data_compression_fn(path: &Path, metadata: &[u8]) -> CompressionAlgorithm {
    if let Ok(loader_type) = get_meta_loader_type_path(metadata) {
        match loader_type.as_str() {
            #[cfg(feature = "brotli")]
            "bevy_render::render_resource::shader::ShaderLoader" => {
                return CompressionAlgorithm::Brotli;
            }
            "bevy_render::texture::image_loader::ImageLoader" => {
                return handle_image_loader(metadata);
            }
            _ => {}
        }
    }

    handle_extensions(path.into())
}

#[inline(always)]
fn handle_image_loader(meta: &[u8]) -> CompressionAlgorithm {
    use bevy::render::texture::{ImageFormat, ImageFormatSetting, ImageLoader};

    match get_meta_loader_settings::<ImageLoader>(meta) {
        Ok(settings) => match settings.format {
            ImageFormatSetting::Format(format) => match format {
                // Don't compress images that already greatly benefits from compression and/or can
                // be decompressed directly by the GPU to avoid unnecessary CPU
                // overhead during asset loading.
                ImageFormat::OpenExr | ImageFormat::Basis | ImageFormat::Ktx2 => {
                    CompressionAlgorithm::None
                }
                _ => DEFAULT_FALLBACK_COMPRESSION_METHOD,
            },
            _ => DEFAULT_FALLBACK_COMPRESSION_METHOD,
        },
        _ => DEFAULT_FALLBACK_COMPRESSION_METHOD,
    }
}

#[inline(always)]
fn handle_extensions(path: PathBuf) -> CompressionAlgorithm {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str().map(|s| s.to_ascii_lowercase()))
        .unwrap_or_default();

    match extension.as_str() {
        "ogg" | "oga" | "spx" | "mp3" | "ktx2" | "exr" | "basis" | "qoi" | "qoa" => {
            CompressionAlgorithm::None
        }
        #[cfg(feature = "brotli")]
        "ron" | "json" | "yml" | "yaml" | "toml" | "txt" | "ini" | "cfg" | "gltf" | "wgsl"
        | "glsl" | "hlsl" | "vert" | "frag" | "vs" | "fs" | "lua" | "js" | "html" | "css"
        | "xml" | "mtlx" | "usda" => CompressionAlgorithm::Brotli,
        _ => DEFAULT_FALLBACK_COMPRESSION_METHOD,
    }
}

pub struct Writer<W: Write> {
    /// The compression method used for entries metadata.
    metadata_compression_method: CompressionAlgorithm,
    /// The callback used to determine the compression method used for entries data.
    /// Takes the file path, a loader's type path and metadata bytes and returns the compression
    /// method.
    data_compression_fn: &'static CompressionMethodFn,
    output: W,
    temp: tempfile::NamedTempFile,
    offset: u64,
    entries: Vec<(String, Entry)>,
    can_write: bool,
}

impl<W: Write> Writer<W> {
    pub(super) fn init(config: WriterBuilder<W>) -> Result<Writer<W>, Error> {
        Ok(Writer {
            metadata_compression_method: config.meta_compression,
            data_compression_fn: config.data_compression_fn,
            output: config.output,
            temp: tempfile::NamedTempFile::new()?,
            offset: Header::SIZE,
            entries: Vec::new(),
            can_write: true,
        })
    }

    /// Add an entry to the archive.
    pub fn add_entry<M, D>(&mut self, path: &Path, meta: &mut M, data: &mut D) -> Result<(), Error>
    where
        M: Read,
        D: Read,
    {
        if !self.can_write {
            return Err(Error::CannotAddEntryAfterFinalize);
        }

        let mut meta_bytes = Vec::new();
        meta.read_to_end(&mut meta_bytes)?;

        let compression_method = (self.data_compression_fn)(path, &meta_bytes);

        let meta_size = self
            .metadata_compression_method
            .compress(&mut meta_bytes.as_slice(), &mut self.temp)? as u64;
        let data_size = compression_method.compress(data, &mut self.temp)? as u64;

        let entry = Entry::new(compression_method, self.offset, meta_size, data_size);
        self.entries
            .push((path.to_string_lossy().to_string(), entry));

        self.offset += meta_size + data_size;

        Ok(())
    }

    /// Finish writing the archive
    pub fn finish(&mut self) -> Result<(), Error> {
        if !self.can_write {
            return Ok(());
        }

        self.can_write = false;
        self.temp.flush()?;

        self.write_header()?;

        self.temp.seek(SeekFrom::Start(0))?;

        // Write entries data.
        std::io::copy(&mut self.temp, &mut self.output)?;

        // Write the entry table.
        self.output.write_all(&self.entries.encode())?;

        Ok(())
    }

    fn write_header(&mut self) -> Result<(), Error> {
        let bytes = Header::new(self.metadata_compression_method, self.offset).encode();
        self.output.write_all(&bytes)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_compression() {
        let mut output = Vec::new();
        let mut writer = WriterBuilder::new(&mut output)
            .meta_compression(CompressionAlgorithm::None)
            .data_compression_fn(&|_, _| CompressionAlgorithm::None)
            .build()
            .unwrap();

        let path_1 = Path::new("ä/b.data");
        let meta_1 = b"meta_1";
        let data_1 = b"data_1";
        let path_2 = Path::new("a/b/c.data");
        let meta_2 = b"meta_2";
        let data_2 = b"data_2";

        writer
            .add_entry(path_1, &mut meta_1.as_slice(), &mut data_1.as_slice())
            .unwrap();
        writer
            .add_entry(path_2, &mut meta_2.as_slice(), &mut data_2.as_slice())
            .unwrap();
        writer.finish().unwrap();

        let mut ground_truth = Vec::new();
        let header = Header::new(
            CompressionAlgorithm::None,
            Header::SIZE + (meta_1.len() + data_1.len() + meta_2.len() + data_2.len()) as u64,
        );

        ground_truth.extend_from_slice(&header.encode());
        ground_truth.extend_from_slice(&meta_1[..]);
        ground_truth.extend_from_slice(&data_1[..]);
        ground_truth.extend_from_slice(&meta_2[..]);
        ground_truth.extend_from_slice(&data_2[..]);
        ground_truth.extend_from_slice(
            &vec![
                (
                    path_1.to_string_lossy().to_string(),
                    Entry::new(
                        CompressionAlgorithm::None,
                        Header::SIZE,
                        meta_1.len() as u64,
                        data_1.len() as u64,
                    ),
                ),
                (
                    path_2.to_string_lossy().to_string(),
                    Entry::new(
                        CompressionAlgorithm::None,
                        Header::SIZE + meta_1.len() as u64 + data_1.len() as u64,
                        meta_2.len() as u64,
                        data_2.len() as u64,
                    ),
                ),
            ]
            .encode(),
        );

        assert_eq!(&ground_truth, &output);
    }

    #[test]
    #[cfg(feature = "deflate")]
    fn test_deflate_compression() {
        let mut output = Vec::new();
        let mut writer = WriterBuilder::new(&mut output)
            .meta_compression(CompressionAlgorithm::Deflate)
            .data_compression_fn(&|_, _| CompressionAlgorithm::Deflate)
            .build()
            .unwrap();

        let path_1 = Path::new("ä/b.data");
        let meta_1 = b"meta_1";
        let data_1 = b"data_1";
        let path_2 = Path::new("a/b/c.data");
        let meta_2 = b"meta_2";
        let data_2 = b"data_2";
        let mut meta_1_compressed = Vec::new();
        let mut data_1_compressed = Vec::new();
        let mut meta_2_compressed = Vec::new();
        let mut data_2_compressed = Vec::new();

        let meta_1_compressed_size = CompressionAlgorithm::Deflate
            .compress(&mut meta_1.as_slice(), &mut meta_1_compressed)
            .unwrap() as u64;
        let data_1_compressed_size = CompressionAlgorithm::Deflate
            .compress(&mut data_1.as_slice(), &mut data_1_compressed)
            .unwrap() as u64;
        let meta_2_compressed_size = CompressionAlgorithm::Deflate
            .compress(&mut meta_2.as_slice(), &mut meta_2_compressed)
            .unwrap() as u64;
        let data_2_compressed_size = CompressionAlgorithm::Deflate
            .compress(&mut data_2.as_slice(), &mut data_2_compressed)
            .unwrap() as u64;

        writer
            .add_entry(path_1, &mut meta_1.as_slice(), &mut data_1.as_slice())
            .unwrap();
        writer
            .add_entry(path_2, &mut meta_2.as_slice(), &mut data_2.as_slice())
            .unwrap();
        writer.finish().unwrap();

        let mut ground_truth = Vec::new();
        let header = Header::new(
            CompressionAlgorithm::Deflate,
            Header::SIZE
                + (meta_1_compressed.len()
                    + data_1_compressed.len()
                    + meta_2_compressed.len()
                    + data_2_compressed.len()) as u64,
        );

        ground_truth.extend_from_slice(&header.encode());
        ground_truth.extend_from_slice(&meta_1_compressed[..]);
        ground_truth.extend_from_slice(&data_1_compressed[..]);
        ground_truth.extend_from_slice(&meta_2_compressed[..]);
        ground_truth.extend_from_slice(&data_2_compressed[..]);
        ground_truth.extend_from_slice(
            &vec![
                (
                    path_1.to_string_lossy().to_string(),
                    Entry::new(
                        CompressionAlgorithm::Deflate,
                        Header::SIZE,
                        meta_1_compressed_size,
                        data_1_compressed_size,
                    ),
                ),
                (
                    path_2.to_string_lossy().to_string(),
                    Entry::new(
                        CompressionAlgorithm::Deflate,
                        Header::SIZE + meta_1_compressed_size + data_1_compressed_size,
                        meta_2_compressed_size,
                        data_2_compressed_size,
                    ),
                ),
            ]
            .encode(),
        );

        assert_eq!(&ground_truth, &output);
    }
}
