use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use bevy::platform::collections::HashMap;

use super::*;
use crate::{Error, Result, encoding::*};

/// Writer for creating HPAK archives.
///
/// This type implements a small builder-style API for configuring how files
/// are added to the archive and how metadata and data are compressed.
pub struct HpakWriter {
    output: File,
    /// Compression method to use for metadata blocks.
    meta_compression: CompressionMethod,
    /// Default compression method to use for files' data when none is provided.
    default_data_compression: CompressionMethod,
    /// Per-extension default compression methods.
    default_compression_by_extension: HashMap<String, CompressionMethod>,
    /// Paths queued to be added to the archive.
    queued_paths: BTreeMap<PathBuf, (PathBuf, Option<CompressionMethod>)>,
    entries: BTreeMap<PathBuf, HpakFileEntry>,
    alignment: Option<u64>,
    /// Whether the metadata should be minified before being written.
    minify_metadata: bool,
    finalized: bool,
}

impl HpakWriter {
    /// Create a new HPAK writer.
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let output = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;

        Ok(Self {
            output,
            meta_compression: CompressionMethod::None,
            default_data_compression: CompressionMethod::None,
            default_compression_by_extension: HashMap::new(),
            queued_paths: BTreeMap::new(),
            entries: BTreeMap::new(),
            alignment: Some(4096),
            finalized: false,
            minify_metadata: true,
        })
    }

    /// Set the compression method used for metadata blocks.
    ///
    /// Defaults to [`CompressionMethod::None`].
    pub fn meta_compression(&mut self, method: CompressionMethod) -> &mut Self {
        self.meta_compression = method;
        self
    }

    /// Set the default compression method for file data when no per-file
    /// override is provided.
    ///
    /// Defaults to [`CompressionMethod::None`].
    pub fn default_data_compression(&mut self, method: CompressionMethod) -> &mut Self {
        self.default_data_compression = method;
        self
    }

    /// Control whether metadata is minified before being written.
    ///
    /// `true` by default.
    pub fn minify_metadata(&mut self, minify: bool) -> &mut Self {
        self.minify_metadata = minify;
        self
    }

    /// Set the alignment for the entries.
    /// Must be a power of two.
    ///
    /// `4096` by default.
    pub fn with_alignment(&mut self, alignment: u64) -> &mut Self {
        if alignment == 0 {
            self.alignment = None;
        } else {
            self.alignment = Some(alignment);
        }

        self
    }

    /// Set the default compression method for a specific file extension.
    ///
    /// If the extension already has a default, it will be overwritten.
    pub fn default_compression_for_extension(
        &mut self,
        extension: &str,
        method: CompressionMethod,
    ) -> &mut Self {
        self.default_compression_by_extension
            .insert(extension.to_string(), method);
        self
    }

    /// Queue a path to be added to the archive using the default compression
    /// strategy.
    pub fn add_path(
        &mut self,
        disk_path: impl AsRef<Path>,
        archive_path: impl AsRef<Path>,
    ) -> &mut Self {
        let key = disk_path.as_ref().to_path_buf();
        self.queued_paths
            .insert(key, (archive_path.as_ref().to_path_buf(), None));
        self
    }

    /// Queue a path with an explicit compression method for its data.
    pub fn add_path_with(
        &mut self,
        disk_path: impl AsRef<Path>,
        archive_path: impl AsRef<Path>,
        compression_method: CompressionMethod,
    ) -> &mut Self {
        let key = disk_path.as_ref().to_path_buf();
        self.queued_paths.insert(
            key,
            (
                archive_path.as_ref().to_path_buf(),
                Some(compression_method),
            ),
        );
        self
    }

    /// Recursively queue all files found under `dir` to be added to the archive.
    pub fn add_paths_from_dir(&mut self, dir: impl AsRef<Path>) -> Result<&mut Self> {
        let dir = dir.as_ref();

        if !dir.exists() {
            return Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("dir directory does not exist: {dir:?}"),
            )));
        }

        if !dir.is_dir() {
            return Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("dir is not a directory: {dir:?}"),
            )));
        }

        for entry in walk_dir(dir) {
            if entry.extension().and_then(|e| e.to_str()).unwrap_or("") == "meta" {
                continue;
            }

            let archive_path = entry.clone();
            let archive_path = match archive_path.strip_prefix(dir) {
                Ok(path) => path,
                Err(e) => {
                    return Err(Error::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("failed to strip prefix: {e}"),
                    )));
                }
            };

            self.add_path(entry, archive_path);
        }

        Ok(self)
    }

    /// Build the archive.
    pub fn build(&mut self) -> Result<()> {
        if self.finalized {
            return Err(Error::AlreadyFinalized);
        }

        // Write dummy header, overwritten in finalize()
        let header = HpakHeader {
            meta_compression_method: CompressionMethod::None,
            entries_offset: 0,
        };
        header.encode(&mut self.output)?;

        for (disk_path, (archive_path, compression_method)) in self.queued_paths.iter().by_ref() {
            let ext = disk_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let meta_path = meta_path_for(disk_path);

            println!("Adding {:?} (meta: {:?})", disk_path, meta_path);
            let meta = File::open(&meta_path)?;
            let data = File::open(disk_path)?;

            let compression_method = compression_method.unwrap_or_else(|| {
                self.default_compression_by_extension
                    .get(ext)
                    .copied()
                    .unwrap_or(CompressionMethod::default())
            });

            let archive_path = archive_path.as_path();

            if self.entries.contains_key(archive_path) {
                return Err(Error::DuplicateEntry(archive_path.to_path_buf()));
            }

            if let Some(alignment) = self.alignment {
                let offset = self.output.stream_position()?;

                let aligned = (offset + (alignment - 1)) & !(alignment - 1);
                let padding = aligned - offset;

                if padding > 0 {
                    let padding_bytes = vec![0u8; padding as usize];
                    self.output.write_all(&padding_bytes)?;
                }

                self.output.flush()?;
            };

            let meta_offset = self.output.stream_position()?;
            let meta_size = self.meta_compression.compress(
                if self.minify_metadata {
                    Box::new(RonMinifier::new(meta)) as Box<dyn Read>
                } else {
                    Box::new(meta) as Box<dyn Read>
                },
                &mut self.output,
            )?;
            let data_size = compression_method.compress(data, &mut self.output)?;

            let entry = HpakFileEntry {
                hash: hash_path(archive_path),
                compression_method,
                meta_offset,
                meta_size,
                data_size,
            };

            self.entries.insert(archive_path.to_path_buf(), entry);
        }

        self.finalize()
    }

    /// Write the entries table and the final header then flush the writer.
    fn finalize(&mut self) -> Result<()> {
        if self.finalized {
            return Ok(());
        }

        self.finalized = true;

        let header = HpakHeader {
            meta_compression_method: self.meta_compression,
            entries_offset: self.output.stream_position()?,
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
}

#[derive(PartialEq, Debug)]
enum RonState {
    None,
    String(RonStringState),
    Comment(RonCommentType),
}

#[derive(PartialEq, Debug)]
enum RonCommentType {
    Line,
    Block,
}

#[derive(PartialEq, Debug)]
enum RonStringState {
    None,
    Escape,
}

pub struct RonMinifier<R: Read> {
    inner: R,
    input_chars: Vec<char>,
    input_pos: usize,
    eof: bool,
    lookahead: Option<char>,
    state: RonState,
    prev: Option<char>,
    prev_input: Option<char>,
    out_buf: Vec<u8>,
}

impl<R: Read> RonMinifier<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            input_chars: Vec::new(),
            input_pos: 0,
            eof: false,
            lookahead: None,
            state: RonState::None,
            prev: None,
            prev_input: None,
            out_buf: Vec::new(),
        }
    }

    fn refill_input_chars(&mut self) -> std::io::Result<bool> {
        if self.input_pos < self.input_chars.len() {
            return Ok(true);
        }

        if self.eof {
            return Ok(false);
        }

        let mut buf = [0u8; 4096];
        let n = self.inner.read(&mut buf)?;
        if n == 0 {
            self.eof = true;
            return Ok(false);
        }

        let s = String::from_utf8_lossy(&buf[..n]).to_string();
        self.input_chars = s.chars().collect();
        self.input_pos = 0;

        Ok(!self.input_chars.is_empty())
    }

    fn peek_char(&mut self) -> std::io::Result<Option<char>> {
        if self.lookahead.is_some() {
            return Ok(self.lookahead);
        }

        if self.input_pos < self.input_chars.len() {
            let c = self.input_chars[self.input_pos];
            self.lookahead = Some(c);
            return Ok(self.lookahead);
        }

        // try to refill
        if self.refill_input_chars()? {
            let c = self.input_chars[self.input_pos];
            self.lookahead = Some(c);
            return Ok(self.lookahead);
        }

        Ok(None)
    }

    fn next_char_consume(&mut self) -> std::io::Result<Option<char>> {
        if let Some(c) = self.lookahead.take() {
            self.input_pos += 1;
            self.prev_input = Some(c);
            return Ok(Some(c));
        }

        if self.input_pos < self.input_chars.len() {
            let c = self.input_chars[self.input_pos];
            self.input_pos += 1;
            self.prev_input = Some(c);
            return Ok(Some(c));
        }

        if self.refill_input_chars()? {
            let c = self.input_chars[self.input_pos];
            self.input_pos += 1;
            self.prev_input = Some(c);
            return Ok(Some(c));
        }

        Ok(None)
    }

    fn push_char(&mut self, c: char) {
        self.prev = Some(c);
        let mut buf = [0u8; 4];
        let s = c.encode_utf8(&mut buf);
        self.out_buf.extend_from_slice(s.as_bytes());
    }
}

impl<R: Read> Read for RonMinifier<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // Fill out_buf until we have something to return or EOF reached
        while self.out_buf.is_empty() {
            let peek = self.peek_char()?;
            if peek.is_none() {
                // no more input and nothing buffered
                return Ok(0);
            }

            let c = peek.unwrap();

            match self.state {
                RonState::String(RonStringState::None) => match c {
                    '\\' => {
                        self.next_char_consume()?;
                        self.state = RonState::String(RonStringState::Escape);
                        self.push_char('\\');
                    }
                    '"' => {
                        self.next_char_consume()?;
                        self.state = RonState::None;
                        self.push_char('"');
                    }
                    _ => {
                        self.next_char_consume()?;
                        self.push_char(c);
                    }
                },
                RonState::String(RonStringState::Escape) => {
                    if let Some(ch) = self.next_char_consume()? {
                        self.push_char(ch);
                    }
                    self.state = RonState::String(RonStringState::None);
                }
                RonState::Comment(RonCommentType::Line) => {
                    if c == '\n' {
                        self.next_char_consume()?;
                        self.state = RonState::None;
                    } else {
                        self.next_char_consume()?;
                    }
                }
                RonState::Comment(RonCommentType::Block) => {
                    if c == '/' && self.prev_input == Some('*') {
                        self.next_char_consume()?;
                        self.state = RonState::None;
                    } else {
                        self.next_char_consume()?;
                    }
                }
                RonState::None => match c {
                    '"' => {
                        self.next_char_consume()?;
                        self.state = RonState::String(RonStringState::None);
                        self.push_char('"');
                    }
                    '/' if self.prev_input == Some('/') => {
                        self.next_char_consume()?;
                        self.state = RonState::Comment(RonCommentType::Line);
                    }
                    '*' if self.prev_input == Some('/') => {
                        self.next_char_consume()?;
                        self.state = RonState::Comment(RonCommentType::Block);
                    }
                    '/' => {
                        self.next_char_consume()?;
                        self.prev_input = Some('/');
                    }
                    _ => {
                        self.next_char_consume()?;
                        if !c.is_ascii_whitespace() {
                            self.push_char(c);
                        } else {
                            let next = if self.input_pos < self.input_chars.len() {
                                Some(self.input_chars[self.input_pos])
                            } else {
                                None
                            };

                            let emit = match (self.prev, next) {
                                (Some(p), Some(n)) => {
                                    (p.is_alphanumeric() && n.is_alphanumeric())
                                        || p == '\\'
                                        || n == '\\'
                                }
                                _ => false,
                            };

                            if emit {
                                self.push_char(c);
                            }
                        }
                    }
                },
            }
        }

        // drain out_buf into caller buffer
        let to_copy = std::cmp::min(buf.len(), self.out_buf.len());
        buf[..to_copy].copy_from_slice(&self.out_buf[..to_copy]);
        // remove drained bytes
        self.out_buf.drain(..to_copy);

        Ok(to_copy)
    }
}

/// Populate `writer` with a set of sensible defaults for common file
/// extensions.
pub fn set_default_extension_compression_methods(writer: &mut HpakWriter) {
    use CompressionMethod::*;

    const EXTENSIONS: [(&str, CompressionMethod); 41] = [
        // audio
        ("ogg", None),
        ("oga", None),
        ("spx", Zlib),
        ("mp3", None),
        ("qoa", None),
        // image
        ("exr", None),
        ("png", None),
        ("jpg", None),
        ("jpeg", None),
        ("webp", None),
        ("ktx", None),
        ("ktx2", None),
        ("basis", None),
        ("qoi", None),
        ("dds", None),
        ("tga", Zlib),
        ("bmp", Zlib),
        // 3d models
        ("gltf", Zlib),
        ("glb", Zlib),
        ("obj", Zlib),
        ("fbx", Zlib),
        ("meshlet_mesh", Zlib),
        // shaders
        ("glsl", Zlib),
        ("hlsl", Zlib),
        ("vert", Zlib),
        ("frag", Zlib),
        ("vs", Zlib),
        ("fs", Zlib),
        ("wgsl", Zlib),
        ("spv", Zlib),
        ("metal", Zlib),
        // text
        ("txt", Zlib),
        ("toml", Zlib),
        ("ron", Zlib),
        ("json", Zlib),
        ("yaml", Zlib),
        ("yml", Zlib),
        ("xml", Zlib),
        ("md", Zlib),
        // video
        ("mp4", Zlib),
        ("webm", None),
    ];

    for (ext, method) in EXTENSIONS {
        writer
            .default_compression_by_extension
            .insert(ext.to_string(), method);
    }
}

#[inline]
fn meta_path_for(path: impl AsRef<Path>) -> PathBuf {
    let mut meta_path = path.as_ref().to_path_buf();
    let mut extension = meta_path.extension().unwrap_or_default().to_os_string();
    extension.push(".meta");
    meta_path.set_extension(extension);
    meta_path
}

fn walk_dir<'a>(root: impl AsRef<Path>) -> Box<dyn Iterator<Item = PathBuf> + 'a> {
    let mut entries = fs::read_dir(root.as_ref())
        .unwrap_or_else(|_| panic!("failed to read directory: {:?}", root.as_ref()))
        .filter_map(|entry| match entry {
            Ok(entry) => {
                let path = entry.path();

                if path.is_dir() {
                    Some(walk_dir(path).collect::<Vec<_>>())
                } else {
                    Some(vec![path])
                }
            }
            Err(_) => None,
        })
        .flatten()
        .collect::<Vec<_>>();

    entries.sort();

    Box::new(entries.into_iter())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read};

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
        let mut min = RonMinifier::new(Cursor::new(input.as_bytes()));
        let mut out = Vec::new();
        min.read_to_end(&mut out).unwrap();
        assert_eq!(output, String::from_utf8(out).unwrap());
    }
}
