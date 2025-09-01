use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use super::*;
use crate::{Error, Result, encoding::*};

pub struct HpakWriter {
    output: File,
    meta_compression_method: CompressionMethod,
    entries: BTreeMap<PathBuf, HpakFileEntry>,
    with_alignment: Option<u64>,
    finalized: bool,
}

impl HpakWriter {
    /// Create a new HPAK writer.
    /// The `meta_compression_method` is used to compress the metadata of the assets.
    /// If `with_padding` is true, padding will be added to align entries to 4096 bytes.
    pub fn new(
        path: impl AsRef<Path>,
        meta_compression_method: CompressionMethod,
        with_alignment: Option<u64>,
    ) -> Result<Self> {
        if let Some(alignment) = with_alignment {
            if alignment & (alignment - 1) != 0 {
                return Err(Error::InvalidAlignment(alignment));
            }
        }

        let mut output = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;

        // write dummy header, overwritten in finalize()
        let header = HpakHeader {
            meta_compression_method,
            entries_offset: 0,
        };
        header.encode(&mut output)?;

        Ok(Self {
            output,
            meta_compression_method,
            entries: BTreeMap::new(),
            with_alignment,
            finalized: false,
        })
    }

    /// Add an entry to the archive.
    pub fn add_entry(
        &mut self,
        path: impl AsRef<Path>,
        mut meta: impl Read,
        data: impl Read,
        compression_method: CompressionMethod,
    ) -> Result<()> {
        if self.finalized {
            return Err(Error::CannotAddEntryAfterFinalize);
        }

        let path = path.as_ref();

        if self.entries.contains_key(path) {
            return Err(Error::DuplicateEntry(path.to_path_buf()));
        }

        self.pad_to_alignment()?;

        let mut meta_str = String::new();
        meta.read_to_string(&mut meta_str)?;

        let meta_offset = self.offset()?;

        let meta_size = self.meta_compression_method.compress(
            std::io::Cursor::new(ron_minify(meta_str.as_str())),
            &mut self.output,
        )?;
        let data_size = compression_method.compress(data, &mut self.output)?;

        let entry = HpakFileEntry {
            hash: hash_path(path),
            compression_method,
            meta_offset,
            meta_size,
            data_size,
        };

        self.entries.insert(path.to_path_buf(), entry);

        Ok(())
    }

    /// Write the entries table and the final header then flush the writer.
    pub fn finalize(&mut self) -> Result<()> {
        if self.finalized {
            return Ok(());
        }

        self.finalized = true;

        self.pad_to_alignment()?;

        let header = HpakHeader {
            meta_compression_method: self.meta_compression_method,
            entries_offset: self.offset()?,
        };

        let mut entries = HpakEntries {
            directories: HashTable::new(),
            files: HashTable::new(),
        };

        // build directory/files tables
        for (path, entry) in self.entries.iter() {
            let mut ancestors = path.ancestors();
            let mut prev = ancestors.next().unwrap().to_path_buf();

            // for each ancestor directory, create or update the directory entry
            for ancestor in ancestors {
                let ancestor_hash = hash_path(ancestor);
                let ancestor: PathBuf = ancestor.into();

                let entry = entries
                    .directories
                    .entry(
                        ancestor_hash,
                        |directory| directory.hash == ancestor_hash,
                        HpakDirectoryEntry::hash,
                    )
                    .or_insert(HpakDirectoryEntry {
                        hash: ancestor_hash,
                        entries: Vec::new(),
                    })
                    .into_mut();

                // add the child entry to the directory
                if entry.entries.iter().all(|path| *path != prev) {
                    entry.entries.push(prev);
                }

                prev = ancestor;
            }

            // add the file entry to the file table
            let path = hash_path(path.as_path());
            entries
                .files
                .insert_unique(path, entry.clone(), HpakFileEntry::hash);
        }

        entries.encode(&mut self.output)?;

        self.output.flush()?;

        // return to the beginning of the file and overwrite dummy header
        self.output.seek(SeekFrom::Start(0))?;
        header.encode(&mut self.output)?;

        self.output.flush()?;

        Ok(())
    }

    #[inline]
    fn offset(&mut self) -> Result<u64> {
        Ok(self.output.stream_position()?)
    }

    fn pad_to_alignment(&mut self) -> Result<()> {
        let alignment = if let Some(alignment) = self.with_alignment {
            alignment
        } else {
            return Ok(());
        };

        let offset = self.offset()?;

        let aligned = (offset + (alignment - 1)) & !(alignment - 1);
        let padding = aligned - offset;

        if padding > 0 {
            let padding_bytes = vec![0u8; padding as usize];
            self.output.write_all(&padding_bytes)?;
        }

        self.output.flush()?;

        Ok(())
    }
}

fn ron_minify(data: &str) -> Vec<u8> {
    #[derive(PartialEq, Debug)]
    enum State {
        None,
        String(StringState),
        Comment(CommentType),
    }

    #[derive(PartialEq, Debug)]
    enum CommentType {
        Line,
        Block,
    }

    #[derive(PartialEq, Debug)]
    enum StringState {
        None,
        Escape,
    }

    let mut output = Vec::with_capacity(data.len());
    let mut data = data.chars().peekable();
    let mut state: State = State::None;
    let mut prev: Option<char> = None;

    macro_rules! push {
        ($c:expr) => {
            prev.replace($c);
            output.push($c);
        };
    }

    while let Some(&c) = data.peek() {
        match state {
            State::String(StringState::None) => match c {
                '\\' => {
                    state = State::String(StringState::Escape);
                    push!(c);
                }
                '"' => {
                    state = State::None;
                    push!(c);
                }
                _ => {
                    push!(c);
                }
            },
            State::String(StringState::Escape) => {
                push!(c);
                state = State::String(StringState::None);
            }
            State::Comment(CommentType::Line) if c == '\n' => {
                state = State::None;
            }
            State::Comment(CommentType::Block) if c == '/' && prev == Some('*') => {
                state = State::None;
            }
            State::None => match c {
                '"' => {
                    state = State::String(StringState::None);
                    push!(c);
                }
                '/' if prev == Some('/') => {
                    state = State::Comment(CommentType::Line);
                }
                '*' if prev == Some('/') => {
                    state = State::Comment(CommentType::Block);
                }
                '/' => {
                    prev.replace(c);
                }
                _ => {
                    if !c.is_ascii_whitespace() {
                        push!(c);
                    }
                }
            },
            _ => {
                prev.replace(c);
            }
        }

        data.next();
    }

    output.into_iter().collect::<String>().into_bytes()
}

/// A set of default compression methods for some extensions.
///
/// | Extension        | Compression Method                 |
/// | ---------------- | ---------------------------------- |
/// | **ogg**          | [`Zlib`](CompressionMethod::Zlib)  |
/// | **oga**          | [`Zlib`](CompressionMethod::Zlib)  |
/// | **spx**          | [`Zlib`](CompressionMethod::Zlib)  |
/// | **mp3**          | [`Zlib`](CompressionMethod::Zlib)  |
/// | **qoa**          | [`Zlib`](CompressionMethod::Zlib)  |
/// | **exr**          | [`None`](CompressionMethod::None)  |
/// | **png**          | [`None`](CompressionMethod::None)  |
/// | **jpg**          | [`None`](CompressionMethod::None)  |
/// | **jpeg**         | [`None`](CompressionMethod::None)  |
/// | **webp**         | [`Zlib`](CompressionMethod::Zlib)  |
/// | **ktx**          | [`None`](CompressionMethod::None)  |
/// | **ktx2**         | [`None`](CompressionMethod::None)  |
/// | **basis**        | [`None`](CompressionMethod::None)  |
/// | **qoi**          | [`Zlib`](CompressionMethod::Zlib)  |
/// | **dds**          | [`None`](CompressionMethod::None)  |
/// | **tga**          | [`Zlib`](CompressionMethod::Zlib)  |
/// | **bmp**          | [`None`](CompressionMethod::None)  |
/// | **gltf**         | [`Zlib`](CompressionMethod::Zlib)  |
/// | **glb**          | [`Zlib`](CompressionMethod::Zlib)  |
/// | **obj**          | [`Zlib`](CompressionMethod::Zlib)  |
/// | **fbx**          | [`Zlib`](CompressionMethod::Zlib)  |
/// | **meshlet_mesh** | [`Zlib`](CompressionMethod::Zlib)  |
/// | **glsl**         | [`Zlib`](CompressionMethod::Zlib)  |
/// | **hlsl**         | [`Zlib`](CompressionMethod::Zlib)  |
/// | **vert**         | [`Zlib`](CompressionMethod::Zlib)  |
/// | **frag**         | [`Zlib`](CompressionMethod::Zlib)  |
/// | **vs**           | [`Zlib`](CompressionMethod::Zlib)  |
/// | **fs**           | [`Zlib`](CompressionMethod::Zlib)  |
/// | **wgsl**         | [`Zlib`](CompressionMethod::Zlib)  |
/// | **spv**          | [`Zlib`](CompressionMethod::Zlib)  |
/// | **metal**        | [`Zlib`](CompressionMethod::Zlib)  |
/// | **txt**          | [`Zlib`](CompressionMethod::Zlib)  |
/// | **toml**         | [`Zlib`](CompressionMethod::Zlib)  |
/// | **ron**          | [`Zlib`](CompressionMethod::Zlib)  |
/// | **json**         | [`Zlib`](CompressionMethod::Zlib)  |
/// | **yaml**         | [`Zlib`](CompressionMethod::Zlib)  |
/// | **yml**          | [`Zlib`](CompressionMethod::Zlib)  |
/// | **xml**          | [`Zlib`](CompressionMethod::Zlib)  |
/// | **md**           | [`Zlib`](CompressionMethod::Zlib)  |
/// | **mp4**          | [`Zlib`](CompressionMethod::Zlib)  |
/// | **webm**         | [`Zlib`](CompressionMethod::Zlib)  |
pub fn default_extensions_compression_method()
-> Option<std::collections::HashMap<String, CompressionMethod>> {
    const DEFAULT_COMPRESSION_METHOD: CompressionMethod = CompressionMethod::Zlib;

    std::collections::HashMap::from([
        // audio
        ("ogg".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("oga".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("spx".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("mp3".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("qoa".to_string(), DEFAULT_COMPRESSION_METHOD),
        // image
        ("exr".to_string(), CompressionMethod::None),
        ("png".to_string(), CompressionMethod::None),
        ("jpg".to_string(), CompressionMethod::None),
        ("jpeg".to_string(), CompressionMethod::None),
        ("webp".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("ktx".to_string(), CompressionMethod::None),
        ("ktx2".to_string(), CompressionMethod::None),
        ("basis".to_string(), CompressionMethod::None),
        ("qoi".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("dds".to_string(), CompressionMethod::None),
        ("tga".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("bmp".to_string(), CompressionMethod::None),
        // 3d models
        ("gltf".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("glb".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("obj".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("fbx".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("meshlet_mesh".to_string(), DEFAULT_COMPRESSION_METHOD),
        // shaders
        ("glsl".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("hlsl".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("vert".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("frag".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("vs".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("fs".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("wgsl".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("spv".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("metal".to_string(), DEFAULT_COMPRESSION_METHOD),
        // text
        ("txt".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("toml".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("ron".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("json".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("yaml".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("yml".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("xml".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("md".to_string(), DEFAULT_COMPRESSION_METHOD),
        // video
        ("mp4".to_string(), DEFAULT_COMPRESSION_METHOD),
        ("webm".to_string(), DEFAULT_COMPRESSION_METHOD),
    ])
    .into()
}

/// Pack all assets presents in the `assets_dir` into a single `output_file` HPAK file.
///
/// The `meta_compression_method` is used to compress the metadata of the assets.
/// The `default_compression_method` is used to compress the data of the assets if no `method`
/// is specified in the `extensions_compression_method` map for the asset's extension.
///
/// If `with_alignment` is Some(N), padding will be added to align entries to N bytes.
pub fn pack_assets_folder(
    assets_dir: impl AsRef<Path>,
    output_file: impl AsRef<Path>,
    meta_compression_method: CompressionMethod,
    default_compression_method: CompressionMethod,
    extensions_compression_method: Option<std::collections::HashMap<String, CompressionMethod>>,
    ignore_missing_meta: bool,
    with_alignment: Option<u64>,
) -> Result<()> {
    let assets_dir = assets_dir.as_ref();
    let extensions_compression_method = extensions_compression_method.as_ref();
    let mut writer = HpakWriter::new(output_file, meta_compression_method, with_alignment)?;

    if !assets_dir.exists() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("assets_dir directory does not exist: {assets_dir:?}"),
        )));
    }

    if !assets_dir.is_dir() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("assets_dir is not a directory: {assets_dir:?}"),
        )));
    }

    for path in walkdir(assets_dir) {
        let extension = path.extension().unwrap_or_default().to_os_string();

        if extension.eq("meta") {
            continue;
        }

        let entry = match path.strip_prefix(assets_dir) {
            Ok(path) => path.to_path_buf(),
            Err(e) => {
                return Err(Error::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid path: {e}"),
                )));
            }
        };

        let meta_path = get_meta_path(&path);

        let mut meta_file: Box<dyn std::io::Read> = if meta_path.exists() {
            Box::new(fs::File::open(&meta_path)?)
        } else if ignore_missing_meta {
            Box::new(std::io::Cursor::new(vec![]))
        } else {
            continue;
        };

        let mut data_file = fs::File::open(&path)?;

        let extension = path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let compression_method = extensions_compression_method
            .and_then(|extensions| extensions.get(&extension).copied())
            .unwrap_or(default_compression_method);

        writer.add_entry(&entry, &mut meta_file, &mut data_file, compression_method)?;
    }

    writer.finalize()?;

    Ok(())
}

#[inline]
fn get_meta_path(path: impl AsRef<Path>) -> PathBuf {
    let mut meta_path = path.as_ref().to_path_buf();
    let mut extension = meta_path.extension().unwrap_or_default().to_os_string();
    extension.push(".meta");
    meta_path.set_extension(extension);
    meta_path
}

fn walkdir<'a>(root: impl AsRef<Path>) -> Box<dyn Iterator<Item = PathBuf> + 'a> {
    Box::new(
        fs::read_dir(root.as_ref())
            .unwrap()
            .filter_map(|entry| match entry {
                Ok(entry) => {
                    let path = entry.path();

                    if path.is_dir() {
                        Some(walkdir(path).collect::<Vec<_>>())
                    } else {
                        Some(vec![path])
                    }
                }
                Err(_) => None,
            })
            .flatten(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::*;

    #[rstest]
    #[case(r#""Hello World!""#, r#""Hello World!""#)]
    #[case(r#""Hello\nWorld!""#, r#""Hello\nWorld!""#)]
    #[case(r#""Hello\ \t World!""#, r#""Hello\ \t World!""#)]
    #[case(
        r#"GameConfig( // optional struct name
    window_size: (800, 600),
    window_title: "PAC-MAN",
    fullscreen: false,

    mouse_sensitivity: 1.4,
    key_bindings: {
        "up": Up,
        "down": Down,
        "left": Left,
        "right": Right,

        // Uncomment to enable WASD controls
        /*
        "W": Up,
        "S": Down,
        "A": Left,
        "D": Right,
        */
    },

    difficulty_options: (
        start_difficulty: Easy,
        adaptive: false,
    ),
)"#,
        "GameConfig(window_size:(800,600),window_title:\"PAC-MAN\",fullscreen:false,mouse_sensitivity:1.4,key_bindings:{\"up\":Up,\"down\":Down,\"left\":Left,\"right\":Right,},difficulty_options:(start_difficulty:Easy,adaptive:false,),)"
    )]
    fn it_minify_ron(#[case] input: &str, #[case] output: &str) {
        assert_eq!(output, String::from_utf8(ron_minify(input)).unwrap());
    }
}
