//! Markdown file decoding and atomic replacement, independent of GPUI state.
//!
//! The editor owns the live buffer and saved rope. Storage accepts a captured
//! text snapshot and returns file data; it never updates a document session.

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Newline {
    Lf,
    CrLf,
    Cr,
}

/// Encoding conventions retained from the opened file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextFormat {
    pub bom: bool,
    pub newline: Newline,
}

/// A successfully decoded file. `text` uses LF; `path` is canonical.
#[derive(Debug)]
pub struct LoadedDocument {
    pub path: PathBuf,
    pub text: String,
    pub format: TextFormat,
}

pub fn open(path: &Path) -> io::Result<LoadedDocument> {
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Choose a .md or .markdown file.",
        ));
    }
    let path = fs::canonicalize(path)?;
    let bytes = fs::read(&path)?;
    let bom = bytes.starts_with(&[0xef, 0xbb, 0xbf]);
    let raw = std::str::from_utf8(if bom { &bytes[3..] } else { &bytes })
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "This file is not valid UTF-8."))?;
    // Mixed endings normalize to the first observed convention on save.
    let newline = match raw.bytes().position(|byte| byte == b'\r' || byte == b'\n') {
        Some(index) if raw.as_bytes()[index..].starts_with(b"\r\n") => Newline::CrLf,
        Some(index) if raw.as_bytes()[index] == b'\r' => Newline::Cr,
        _ => Newline::Lf,
    };
    Ok(LoadedDocument {
        path,
        text: normalize_newlines(raw),
        format: TextFormat { bom, newline },
    })
}

/// Normalize disk text for editor and search source-byte coordinates.
pub fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Write beside the destination and replace it only after the full write syncs.
/// This primitive retains the existing replacement behavior; conflict detection
/// and the supported metadata policy are separate storage responsibilities.
pub fn save_file(path: &Path, text: &str, format: TextFormat) -> io::Result<()> {
    let mut temporary = tempfile::NamedTempFile::new_in(path.parent().unwrap_or(Path::new(".")))?;
    if format.bom {
        temporary.write_all(&[0xef, 0xbb, 0xbf])?;
    }
    let normalized = normalize_newlines(text);
    let encoded = match format.newline {
        Newline::Lf => normalized,
        Newline::CrLf => normalized.replace('\n', "\r\n"),
        Newline::Cr => normalized.replace('\n', "\r"),
    };
    temporary.write_all(encoded.as_bytes())?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}
