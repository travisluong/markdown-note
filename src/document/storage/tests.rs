use super::*;
use crate::document;

fn fixture() -> (tempfile::TempDir, document::LoadedDocument) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("note.md");
    fs::write(&path, "original\n").unwrap();
    let loaded = document::open(&path).unwrap();
    (directory, loaded)
}

fn replacement(path: &Path, bytes: &[u8]) {
    let mut file = tempfile::NamedTempFile::new_in(path.parent().unwrap()).unwrap();
    file.write_all(bytes).unwrap();
    file.persist(path).unwrap();
}

#[test]
fn external_edit_after_staging_conflicts_and_removes_only_our_temporary() {
    let (directory, loaded) = fixture();
    let result = save_with_hooks(
        &loaded.path,
        "ours\n",
        loaded.format,
        &loaded.version,
        || fs::write(&loaded.path, "external\n"),
        platform::identity,
    );
    assert!(matches!(
        result,
        Err(SaveError::Conflict(Conflict::ContentChanged))
    ));
    assert_eq!(fs::read(&loaded.path).unwrap(), b"external\n");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn same_bytes_replacement_after_staging_conflicts_by_file_identity() {
    let (directory, loaded) = fixture();
    assert!(loaded.version.identity.is_some());
    let result = save_with_hooks(
        &loaded.path,
        "ours\n",
        loaded.format,
        &loaded.version,
        || {
            replacement(&loaded.path, b"original\n");
            Ok(())
        },
        platform::identity,
    );
    assert!(matches!(
        result,
        Err(SaveError::Conflict(Conflict::Replaced))
    ));
    assert_eq!(fs::read(&loaded.path).unwrap(), b"original\n");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn staging_failure_keeps_original_and_cleans_up() {
    let (directory, loaded) = fixture();
    let result = save_with_hooks(
        &loaded.path,
        "ours\n",
        loaded.format,
        &loaded.version,
        || Err(io::Error::other("injected failure before replacement")),
        platform::identity,
    );
    assert!(matches!(result, Err(SaveError::Io(_))));
    assert_eq!(fs::read(&loaded.path).unwrap(), b"original\n");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn receipt_remembers_our_handle_when_destination_is_replaced_after_commit() {
    let (_directory, loaded) = fixture();
    let receipt = save_with_hooks(
        &loaded.path,
        "ours\n",
        loaded.format,
        &loaded.version,
        || Ok(()),
        |committed| {
            replacement(&loaded.path, b"ours\n");
            platform::identity(committed)
        },
    )
    .unwrap();
    assert_eq!(receipt.version.raw.as_ref(), b"ours\n");
    assert!(matches!(
        save_file(&loaded.path, "later\n", loaded.format, &receipt.version),
        Err(SaveError::Conflict(Conflict::Replaced))
    ));
    assert_eq!(fs::read(&loaded.path).unwrap(), b"ours\n");
}

#[cfg(windows)]
#[test]
fn receipt_handle_blocks_replacement_until_observed_then_detects_a_replacement() {
    let (directory, loaded) = fixture();
    let receipt = save_with_hooks(
        &loaded.path,
        "ours\n",
        loaded.format,
        &loaded.version,
        || Ok(()),
        |committed| {
            let mut external = tempfile::NamedTempFile::new_in(directory.path())?;
            external.write_all(b"ours\n")?;
            // MoveFileExW cannot replace this open destination, even though the
            // receipt handle permits delete sharing. The failed staged file is
            // removed when its PersistError is dropped.
            let error = external.persist(&loaded.path).unwrap_err();
            assert_eq!(error.error.kind(), io::ErrorKind::PermissionDenied);
            platform::identity(committed)
        },
    )
    .unwrap();
    assert_eq!(receipt.version.raw.as_ref(), b"ours\n");
    assert!(receipt.version.identity.is_some());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    // The receipt observation closes its handle before returning to the caller.
    // A same-byte replacement is now possible and must invalidate that receipt.
    replacement(&loaded.path, b"ours\n");
    assert!(matches!(
        save_file(&loaded.path, "later\n", loaded.format, &receipt.version),
        Err(SaveError::Conflict(Conflict::Replaced))
    ));
    assert_eq!(fs::read(&loaded.path).unwrap(), b"ours\n");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}
#[test]
fn receipt_never_adopts_an_external_in_place_edit_after_commit() {
    let (_directory, loaded) = fixture();
    let receipt = save_with_hooks(
        &loaded.path,
        "ours\n",
        loaded.format,
        &loaded.version,
        || Ok(()),
        |committed| {
            fs::write(&loaded.path, "external\n")?;
            platform::identity(committed)
        },
    )
    .unwrap();
    assert_eq!(receipt.version.raw.as_ref(), b"ours\n");
    assert!(matches!(
        save_file(&loaded.path, "later\n", loaded.format, &receipt.version),
        Err(SaveError::Conflict(Conflict::ContentChanged))
    ));
}

#[test]
fn failed_post_commit_identity_observation_still_returns_the_written_baseline() {
    let (_directory, loaded) = fixture();
    let receipt = save_with_hooks(
        &loaded.path,
        "ours\n",
        loaded.format,
        &loaded.version,
        || Ok(()),
        |_| Err(io::Error::other("injected identity query failure")),
    )
    .unwrap();
    assert_eq!(fs::read(&loaded.path).unwrap(), b"ours\n");
    assert_eq!(receipt.version.raw.as_ref(), b"ours\n");
    assert!(receipt.version.identity.is_none());
    fs::write(&loaded.path, "external\n").unwrap();
    assert!(matches!(
        save_file(&loaded.path, "later\n", loaded.format, &receipt.version),
        Err(SaveError::Conflict(Conflict::ContentChanged))
    ));
}

#[cfg(unix)]
#[test]
fn changed_parent_does_not_redirect_commit_or_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let parent = directory.path().join("notes");
    let moved = directory.path().join("moved");
    fs::create_dir(&parent).unwrap();
    let path = parent.join("note.md");
    fs::write(&path, "original\n").unwrap();
    let loaded = document::open(&path).unwrap();
    let result = save_with_hooks(
        &loaded.path,
        "ours\n",
        loaded.format,
        &loaded.version,
        || {
            fs::rename(&parent, &moved)?;
            fs::create_dir(&parent)?;
            fs::write(&path, "replacement directory\n")
        },
        platform::identity,
    );
    assert!(matches!(
        result,
        Err(SaveError::Conflict(Conflict::UnsafePath))
    ));
    assert_eq!(fs::read(&path).unwrap(), b"replacement directory\n");
    assert_eq!(fs::read(moved.join("note.md")).unwrap(), b"original\n");
    assert_eq!(fs::read_dir(&parent).unwrap().count(), 1);
    assert_eq!(
        fs::read_dir(&moved).unwrap().count(),
        2,
        "staged file is retained in its moved directory"
    );
}

#[cfg(unix)]
#[test]
fn parent_link_introduced_during_staging_is_refused() {
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir().unwrap();
    let parent = directory.path().join("notes");
    let moved = directory.path().join("moved");
    let other = directory.path().join("other");
    fs::create_dir(&parent).unwrap();
    fs::create_dir(&other).unwrap();
    fs::write(parent.join("note.md"), "original\n").unwrap();
    fs::write(other.join("note.md"), "external\n").unwrap();
    let loaded = document::open(&parent.join("note.md")).unwrap();
    let result = save_with_hooks(
        &loaded.path,
        "ours\n",
        loaded.format,
        &loaded.version,
        || {
            fs::rename(&parent, &moved)?;
            symlink(&other, &parent)
        },
        platform::identity,
    );
    assert!(matches!(
        result,
        Err(SaveError::Conflict(Conflict::UnsafePath))
    ));
    assert_eq!(fs::read(other.join("note.md")).unwrap(), b"external\n");
    assert_eq!(fs::read_dir(&other).unwrap().count(), 1);
}

#[test]
fn missing_identity_cannot_silently_weaken_a_known_preflight_baseline() {
    let (_directory, loaded) = fixture();
    let unavailable = DiskVersion {
        raw: loaded.version.raw.clone(),
        identity: None,
    };
    assert!(matches!(
        compare(&loaded.version, &unavailable),
        Err(SaveError::Conflict(Conflict::IdentityUnavailable))
    ));
}

#[cfg(windows)]
#[test]
fn open_staged_file_blocks_parent_rename_and_failed_attempt_cleans_up() {
    let directory = tempfile::tempdir().unwrap();
    let parent = directory.path().join("notes");
    let moved = directory.path().join("moved");
    fs::create_dir(&parent).unwrap();
    fs::write(parent.join("note.md"), "original\n").unwrap();
    let loaded = document::open(&parent.join("note.md")).unwrap();
    let result = save_with_hooks(
        &loaded.path,
        "ours\n",
        loaded.format,
        &loaded.version,
        || fs::rename(&parent, &moved),
        platform::identity,
    );
    // Windows cannot rename a directory containing the open staged file, even
    // with delete sharing. The failed external attempt must not commit or leak it.
    assert!(
        matches!(result, Err(SaveError::Io(error)) if error.kind() == io::ErrorKind::PermissionDenied)
    );
    assert_eq!(fs::read(&loaded.path).unwrap(), b"original\n");
    assert!(!moved.exists());
    assert_eq!(fs::read_dir(&parent).unwrap().count(), 1);
}

#[cfg(windows)]
#[test]
fn parent_junction_is_rejected_by_initial_and_repeated_path_validation() {
    use std::{os::windows::process::CommandExt, process::Command};
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let directory = tempfile::tempdir().unwrap();
    let parent = directory.path().join("notes");
    let moved = directory.path().join("moved");
    let other = directory.path().join("other");
    fs::create_dir(&parent).unwrap();
    fs::create_dir(&other).unwrap();
    fs::write(parent.join("note.md"), "original\n").unwrap();
    fs::write(other.join("note.md"), "external\n").unwrap();
    let loaded = document::open(&parent.join("note.md")).unwrap();
    let guard = ParentGuard::capture(loaded.path.parent().unwrap()).unwrap();
    // Introduce the junction before opening a staged file: Windows itself blocks
    // parent-directory renames while a file within that directory remains open.
    fs::rename(&parent, &moved).unwrap();
    // Paths are environment data, never interpolated into command text.
    let script = "New-Item -ItemType Junction -Path $env:GPUI_DOCUMENT_TEST_LINK -Target $env:GPUI_DOCUMENT_TEST_TARGET | Out-Null";
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("GPUI_DOCUMENT_TEST_LINK", &parent)
        .env("GPUI_DOCUMENT_TEST_TARGET", &other)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .unwrap();
    let is_junction =
        fs::symlink_metadata(&parent).is_ok_and(|metadata| platform::is_link(&metadata));
    let revalidated = guard.check();
    let result = save_file(&loaded.path, "ours\n", loaded.format, &loaded.version);
    // Cleanup must never try to remove a real directory after failed setup.
    if is_junction {
        fs::remove_dir(&parent).unwrap();
    }
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(is_junction, "fixture must exercise an actual reparse point");
    assert!(matches!(
        revalidated,
        Err(SaveError::Conflict(Conflict::UnsafePath))
    ));
    assert!(matches!(
        result,
        Err(SaveError::Conflict(Conflict::UnsafePath))
    ));
    assert_eq!(fs::read(other.join("note.md")).unwrap(), b"external\n");
    assert_eq!(fs::read_dir(&other).unwrap().count(), 1);
    assert_eq!(fs::read(moved.join("note.md")).unwrap(), b"original\n");
    assert_eq!(fs::read_dir(&moved).unwrap().count(), 1);
}
#[cfg(unix)]
#[test]
fn permission_change_during_staging_is_preserved_on_the_replacement() {
    use std::os::unix::fs::PermissionsExt;
    let (_directory, loaded) = fixture();
    fs::set_permissions(&loaded.path, fs::Permissions::from_mode(0o600)).unwrap();
    save_with_hooks(
        &loaded.path,
        "ours\n",
        loaded.format,
        &loaded.version,
        || fs::set_permissions(&loaded.path, fs::Permissions::from_mode(0o640)),
        platform::identity,
    )
    .unwrap();
    assert_eq!(fs::read(&loaded.path).unwrap(), b"ours\n");
    assert_eq!(
        fs::metadata(&loaded.path).unwrap().permissions().mode() & 0o7777,
        0o640
    );
}
