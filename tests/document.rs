//! Exercise the storage API through the library, without importing the app root.
use gpui_note::document::{self, Newline, TextFormat};
use std::{fs, io};

#[test]
fn bom_and_newline_round_trip_through_the_public_storage_api() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("note.md");
    for (text, newline) in [
        ("# Café\n你好\n", Newline::Lf),
        ("# Café\r\n你好\r\n", Newline::CrLf),
        ("# Café\r你好\r", Newline::Cr),
        ("", Newline::Lf),
    ] {
        for bom in [false, true] {
            let original = [
                if bom { b"\xef\xbb\xbf".as_slice() } else { b"" },
                text.as_bytes(),
            ]
            .concat();
            fs::write(&path, &original).unwrap();
            let loaded = document::open(&path).unwrap();
            assert_eq!(loaded.path, fs::canonicalize(&path).unwrap());
            assert_eq!(loaded.format, TextFormat { bom, newline });
            document::save_file(&loaded.path, &loaded.text, loaded.format).unwrap();
            assert_eq!(fs::read(&path).unwrap(), original);

            let edited = format!("{}edit\n", loaded.text);
            document::save_file(&loaded.path, &edited, loaded.format).unwrap();
            assert_eq!(document::open(&path).unwrap().text, edited);
        }
    }
}

#[test]
fn mixed_endings_use_the_first_observed_convention() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mixed.markdown");
    for (original, newline, expected) in [
        ("a\nb\r\nc\r", Newline::Lf, "a\nb\nc\n"),
        ("a\r\nb\rc\n", Newline::CrLf, "a\r\nb\r\nc\r\n"),
        ("a\rb\nc\r\n", Newline::Cr, "a\rb\rc\r"),
    ] {
        fs::write(&path, original).unwrap();
        let loaded = document::open(&path).unwrap();
        assert_eq!(loaded.text, "a\nb\nc\n");
        assert_eq!(loaded.format.newline, newline);
        document::save_file(&loaded.path, &loaded.text, loaded.format).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), expected);
    }
}

#[test]
fn opening_rejects_invalid_utf8_missing_files_and_unsupported_extensions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("note.md");
    fs::write(&path, [0xff]).unwrap();
    assert_eq!(
        document::open(&path).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        document::open(&directory.path().join("missing.md"))
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotFound,
    );
    assert_eq!(
        document::open(&directory.path().join("file.txt"))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput,
    );
}

#[cfg(windows)]
#[test]
fn sharing_locked_save_preserves_original_and_cleans_up_temporary_file() {
    use std::os::windows::fs::OpenOptionsExt;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("note.md");
    fs::write(&path, "original").unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    assert!(
        document::save_file(
            &path,
            "changed",
            TextFormat {
                bom: false,
                newline: Newline::Lf
            }
        )
        .is_err()
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "original");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    drop(lock);
}
