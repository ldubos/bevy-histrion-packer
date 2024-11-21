use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use super::*;
use crate::{encoding::*, Error, Result};

pub struct HpakWriter {
    output: File,
    meta_compression_method: CompressionMethod,
    entries: BTreeMap<PathBuf, HpakFileEntry>,
    with_padding: bool,
    finalized: bool,
}

impl HpakWriter {
    /// Create a new HPAK writer.
    /// The `meta_compression_method` is used to compress the metadata of the assets.
    /// If `with_padding` is true, padding will be added to align entries to 4096 bytes.
    pub fn new(
        path: impl AsRef<Path>,
        meta_compression_method: CompressionMethod,
        with_padding: bool,
    ) -> Result<Self> {
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
            with_padding,
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
        if !self.with_padding {
            return Ok(());
        }

        const ALIGNMENT: u64 = 4096;

        let offset = self.offset()?;

        let aligned = (offset + (ALIGNMENT - 1)) & !(ALIGNMENT - 1);
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
    #[derive(Debug, PartialEq)]
    enum State {
        None,
        String(StringState),
        Comment(CommentType),
    }

    #[derive(Debug, PartialEq)]
    enum CommentType {
        Line,
        Block,
    }

    #[derive(Debug, PartialEq)]
    enum StringState {
        None,
        Escape,
    }

    let mut output = Vec::new();
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
                }
                '"' => {
                    state = State::None;
                    push!(c);
                }
                _ => {
                    push!(c);
                }
            },
            State::String(StringState::Escape) => match c {
                't' => {
                    push!('\t');
                    state = State::String(StringState::None);
                }
                _ => {
                    push!('\\');
                    push!(c);
                    state = State::String(StringState::None);
                }
            },
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

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::*;

    #[rstest]
    #[case(r#""Hello World!""#, r#""Hello World!""#)]
    #[case(r#""Hello\nWorld!""#, r#""Hello\nWorld!""#)]
    #[case(r#""Hello\ \t World!""#, "\"Hello\\ \t World!\"")]
    #[case(r#"GameConfig( // optional struct name
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
)"#, "GameConfig(window_size:(800,600),window_title:\"PAC-MAN\",fullscreen:false,mouse_sensitivity:1.4,key_bindings:{\"up\":Up,\"down\":Down,\"left\":Left,\"right\":Right,},difficulty_options:(start_difficulty:Easy,adaptive:false,),)")]
    fn it_minify_ron(#[case] input: &str, #[case] output: &str) {
        assert_eq!(output, String::from_utf8(ron_minify(input)).unwrap());
    }
}
