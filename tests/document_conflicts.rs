use markdown_note::document::{self, Conflict, SaveError};
use std::{fs, io, io::Write};

fn fixture(bytes: &[u8]) -> (tempfile::TempDir, document::LoadedDocument) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("note.md");
    fs::write(&path, bytes).unwrap();
    let loaded = document::open(&path).unwrap();
    (directory, loaded)
}

#[test]
fn byte_guard_detects_same_length_invalid_utf8_and_format_only_changes() {
    for (original, changed) in [
        (b"first\n".as_slice(), b"other\n".as_slice()),
        (b"text\n".as_slice(), b"text\xff".as_slice()),
        (b"text\n".as_slice(), b"\xef\xbb\xbftext\n".as_slice()),
        (b"text\r\n".as_slice(), b"text\n".as_slice()),
    ] {
        let (directory, loaded) = fixture(original);
        fs::write(&loaded.path, changed).unwrap();
        assert!(matches!(
            document::save_file(&loaded.path, "ours\n", loaded.format, &loaded.version),
            Err(SaveError::Conflict(Conflict::ContentChanged))
        ));
        assert_eq!(fs::read(&loaded.path).unwrap(), changed);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}

#[test]
fn replacement_with_identical_bytes_is_a_conflict() {
    let (_directory, loaded) = fixture(b"original\n");
    assert!(loaded.version.identity_available());
    let mut replacement = tempfile::NamedTempFile::new_in(loaded.path.parent().unwrap()).unwrap();
    replacement.write_all(b"original\n").unwrap();
    replacement.persist(&loaded.path).unwrap();
    assert!(matches!(
        document::save_file(&loaded.path, "ours\n", loaded.format, &loaded.version),
        Err(SaveError::Conflict(Conflict::Replaced))
    ));
    assert_eq!(fs::read(&loaded.path).unwrap(), b"original\n");
}

#[test]
fn deleted_file_is_not_recreated_and_directory_is_not_replaced() {
    let (_directory, loaded) = fixture(b"original\n");
    fs::remove_file(&loaded.path).unwrap();
    assert!(matches!(
        document::save_file(&loaded.path, "ours\n", loaded.format, &loaded.version),
        Err(SaveError::Conflict(Conflict::Missing))
    ));
    assert!(!loaded.path.exists());
    fs::create_dir(&loaded.path).unwrap();
    assert!(matches!(
        document::save_file(&loaded.path, "ours\n", loaded.format, &loaded.version),
        Err(SaveError::Conflict(Conflict::UnsafePath))
    ));
    assert!(loaded.path.is_dir());
}

#[test]
fn readonly_file_is_refused_even_when_its_parent_is_writable() {
    let (directory, loaded) = fixture(b"original\n");
    let writable = fs::metadata(&loaded.path).unwrap().permissions();
    let mut readonly = writable.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&loaded.path, readonly).unwrap();
    let result = document::save_file(&loaded.path, "ours\n", loaded.format, &loaded.version);
    // Restore the exact original permissions so temporary-directory cleanup is safe on Windows.
    fs::set_permissions(&loaded.path, writable).unwrap();
    assert!(
        matches!(result, Err(SaveError::Io(error)) if error.kind() == io::ErrorKind::PermissionDenied)
    );
    assert_eq!(fs::read(&loaded.path).unwrap(), b"original\n");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn successful_receipt_is_required_for_the_next_save() {
    let (_directory, loaded) = fixture(b"original\n");
    let receipt =
        document::save_file(&loaded.path, &loaded.text, loaded.format, &loaded.version).unwrap();
    assert!(matches!(
        document::save_file(&loaded.path, "later\n", loaded.format, &loaded.version),
        Err(SaveError::Conflict(Conflict::Replaced))
    ));
    let receipt =
        document::save_file(&loaded.path, "later\n", loaded.format, &receipt.version).unwrap();
    document::save_file(&loaded.path, "latest\n", loaded.format, &receipt.version).unwrap();
    assert_eq!(fs::read(&loaded.path).unwrap(), b"latest\n");
}

#[test]
fn explicit_move_revalidation_keeps_saved_bytes_and_rejects_external_changes() {
    let (_directory, loaded) = fixture(b"original\n");
    let moved = loaded.path.parent().unwrap().join("moved.md");
    fs::rename(&loaded.path, &moved).unwrap();
    let version = document::version_after_move(&moved, &loaded.version).unwrap();
    let receipt = document::save_file(&moved, "ours\n", loaded.format, &version).unwrap();
    fs::write(&moved, "external\n").unwrap();
    assert!(matches!(
        document::version_after_move(&moved, &receipt.version),
        Err(SaveError::Conflict(Conflict::ContentChanged))
    ));
}

#[cfg(unix)]
#[test]
fn unix_permissions_use_current_destination_mode_without_changing_ownership_policy() {
    use std::os::unix::fs::PermissionsExt;
    for mode in [0o640, 0o750] {
        let (_directory, loaded) = fixture(b"original\n");
        fs::set_permissions(&loaded.path, fs::Permissions::from_mode(mode)).unwrap();
        document::save_file(&loaded.path, "ours\n", loaded.format, &loaded.version).unwrap();
        assert_eq!(
            fs::metadata(&loaded.path).unwrap().permissions().mode() & 0o7777,
            mode
        );
    }
}

#[cfg(unix)]
#[test]
fn selected_symlink_opens_canonical_target_but_new_leaf_link_is_refused() {
    use std::os::unix::fs::symlink;
    let (directory, loaded) = fixture(b"original\n");
    let alias = directory.path().join("alias.md");
    symlink(&loaded.path, &alias).unwrap();
    let through_alias = document::open(&alias).unwrap();
    assert_eq!(through_alias.path, loaded.path);
    document::save_file(
        &through_alias.path,
        "ours\n",
        through_alias.format,
        &through_alias.version,
    )
    .unwrap();
    let current = document::open(&loaded.path).unwrap();
    let other = directory.path().join("other.md");
    fs::write(&other, "external\n").unwrap();
    fs::remove_file(&loaded.path).unwrap();
    symlink(&other, &loaded.path).unwrap();
    assert!(matches!(
        document::save_file(&loaded.path, "latest\n", current.format, &current.version),
        Err(SaveError::Conflict(Conflict::UnsafePath))
    ));
    assert_eq!(fs::read(&other).unwrap(), b"external\n");
}

#[cfg(unix)]
#[test]
fn saving_does_not_require_permission_to_list_the_parent_directory() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let parent = directory.path().join("write-only");
    fs::create_dir(&parent).unwrap();
    let path = parent.join("note.md");
    fs::write(&path, "original\n").unwrap();
    let loaded = document::open(&path).unwrap();
    let original_mode = fs::metadata(&parent).unwrap().permissions();
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o300)).unwrap();
    let result = document::save_file(&loaded.path, "ours\n", loaded.format, &loaded.version);
    fs::set_permissions(&parent, original_mode).unwrap();
    result.unwrap();
    assert_eq!(fs::read(&loaded.path).unwrap(), b"ours\n");
    assert_eq!(fs::read_dir(&parent).unwrap().count(), 1);
}
