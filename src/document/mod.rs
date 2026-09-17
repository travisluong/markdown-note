//! UTF-8 text file decoding and atomic replacement, independent of GPUI state.
//!
//! The editor owns the live buffer and saved rope. Storage accepts a captured
//! text snapshot and returns file data; it never updates a document session.

mod platform;
mod storage;
pub use storage::{Conflict, SaveError, WriteReceipt, save_file, version_after_move};

use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Opaque evidence captured from an opened file or a successful write. Raw bytes
/// include the BOM and original newline spellings, independently of editor text.
#[derive(Clone, PartialEq, Eq)]
pub struct DiskVersion {
    raw: Arc<[u8]>,
    identity: Option<platform::FileIdentity>,
}
impl DiskVersion {
    /// Some filesystems do not expose identity. Raw-byte comparison still applies.
    pub fn identity_available(&self) -> bool {
        self.identity.is_some()
    }
}
impl fmt::Debug for DiskVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DiskVersion")
            .field("bytes", &self.raw.len())
            .field("identity", &self.identity)
            .finish()
    }
}

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
    pub version: DiskVersion,
}

pub fn open(path: &Path) -> io::Result<LoadedDocument> {
    let path = fs::canonicalize(path)?;
    let captured = storage::capture(&path)?;
    let bytes: &[u8] = captured.version.raw.as_ref();
    let bom = bytes.starts_with(&[0xef, 0xbb, 0xbf]);
    let raw = std::str::from_utf8(if bom { &bytes[3..] } else { bytes })
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "This file is not valid UTF-8."))?;
    if raw
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t' | '\u{c}'))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "This file contains binary data. Open a UTF-8 text file instead.",
        ));
    }
    // Mixed endings normalize to the first observed convention on save.
    let newline = match raw.bytes().position(|byte| byte == b'\r' || byte == b'\n') {
        Some(index) if raw.as_bytes()[index..].starts_with(b"\r\n") => Newline::CrLf,
        Some(index) if raw.as_bytes()[index] == b'\r' => Newline::Cr,
        _ => Newline::Lf,
    };
    let text = normalize_newlines(raw);
    Ok(LoadedDocument {
        path,
        text,
        format: TextFormat { bom, newline },
        version: captured.version,
    })
}

/// Normalize disk text for editor and search source-byte coordinates.
pub fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Encode a captured editor snapshot using its existing disk conventions.
pub fn encode(text: &str, format: TextFormat) -> Vec<u8> {
    let normalized = normalize_newlines(text);
    let encoded = match format.newline {
        Newline::Lf => normalized,
        Newline::CrLf => normalized.replace('\n', "\r\n"),
        Newline::Cr => normalized.replace('\n', "\r"),
    };
    let mut bytes = Vec::with_capacity(encoded.len() + usize::from(format.bom) * 3);
    if format.bom {
        bytes.extend_from_slice(&[0xef, 0xbb, 0xbf]);
    }
    bytes.extend_from_slice(encoded.as_bytes());
    bytes
}
