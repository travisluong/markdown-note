//! Deterministic queue transitions and disposable filesystem writes; no GPUI window.

use std::{fs, io, path::PathBuf};

use markdown_note::notebook::{
    FlushStatus, Index, Notebook, Settings, SettingsWriter, WriteJob, load_index, save_book,
    save_index, settings_path,
};

// Synthetic destinations exercise the pure model without touching the filesystem.
fn book(name: &str, edit: usize) -> Notebook {
    Notebook {
        root: PathBuf::from(name),
        settings: Settings {
            date_format: format!("{name}-{edit}"),
            ..Settings::default()
        },
    }
}

fn index_for(book: &Notebook) -> Index {
    let mut index = Index::default();
    index.register(book.root.clone());
    index
}

fn submit(writer: &mut SettingsWriter, book: Notebook) {
    writer.submit(index_for(&book), Some(book));
}

fn success(writer: &mut SettingsWriter, job: WriteJob) {
    assert!(writer.complete(job.id(), Ok(())));
}

fn failure(writer: &mut SettingsWriter, job: WriteJob) {
    assert!(writer.complete(job.id(), Err(io::Error::other("blocked destination"))));
}

#[test]
fn one_running_job_and_latest_snapshots_are_bounded_under_two_book_stress() {
    let mut writer = SettingsWriter::new("index.json".into());
    let first = writer.submit(index_for(&book("A", 0)), Some(book("A", 0)));
    let running = writer.take_job().unwrap();
    let early_barrier = writer.flush_all();
    assert_eq!(running.revision(), first);
    assert!(writer.take_job().is_none());
    for edit in 1..=10_000 {
        submit(&mut writer, book("A", edit));
        submit(&mut writer, book("B", edit));
        assert!(writer.retained_snapshot_count() <= 3);
        assert!(writer.queued_destination_count() <= 3);
        assert!(writer.take_job().is_none());
    }
    assert_eq!(running.notebook().unwrap().settings.date_format, "A-0");
    success(&mut writer, running);
    let barrier = writer.flush_all();
    let mut books = Vec::new();
    let mut indexes = Vec::new();
    while let Some(job) = writer.take_job() {
        assert!(job.revision() > first);
        if let Some(book) = job.notebook() {
            books.push(book.settings.date_format.clone());
        } else {
            indexes.push(job.index().unwrap().active.clone());
        }
        success(&mut writer, job);
    }
    books.sort();
    assert_eq!(books, ["A-10000", "B-10000"]);
    assert_eq!(indexes, [Some(PathBuf::from("B"))]);
    assert_eq!(writer.flush_status(&early_barrier), FlushStatus::Succeeded);
    assert_eq!(writer.flush_status(&barrier), FlushStatus::Succeeded);
    assert_eq!(writer.retained_snapshot_count(), 0);
    assert_eq!(writer.queued_destination_count(), 0);
}

#[test]
fn dirty_destination_order_is_fair_even_when_one_book_keeps_changing() {
    let mut writer = SettingsWriter::new("index.json".into());
    submit(&mut writer, book("A", 0));
    let first = writer.take_job().unwrap();
    submit(&mut writer, book("B", 1));
    for edit in 1..=100 {
        submit(&mut writer, book("A", edit));
    }
    success(&mut writer, first);
    let next = writer.take_job().unwrap();
    assert_eq!(next.notebook().unwrap().root, PathBuf::from("B"));
    success(&mut writer, next);
    let next = writer.take_job().unwrap();
    assert_eq!(next.notebook().unwrap().settings.date_format, "A-100");
    success(&mut writer, next);
    let index = writer.take_job().unwrap();
    assert_eq!(index.index().unwrap().active, Some(PathBuf::from("A")));
    success(&mut writer, index);
}

#[test]
fn stale_completions_cannot_release_a_job_or_retire_newer_snapshots() {
    let mut writer = SettingsWriter::new("index.json".into());
    submit(&mut writer, book("A", 1));
    let first = writer.take_job().unwrap();
    let stale = first.id();
    submit(&mut writer, book("A", 2));
    success(&mut writer, first);
    let current = writer.take_job().unwrap();
    assert_eq!(current.notebook().unwrap().settings.date_format, "A-2");
    assert!(!writer.complete(stale, Ok(())));
    assert!(!writer.complete(stale, Err(io::Error::other("late failure"))));
    assert!(writer.is_running());
    assert!(writer.take_job().is_none());
    success(&mut writer, current);
    let index = writer.take_job().unwrap();
    success(&mut writer, index);
    assert_eq!(
        writer.flush_status(&writer.flush_all()),
        FlushStatus::Succeeded
    );
}

#[test]
fn an_old_failed_attempt_allows_the_newer_snapshot_to_run_once() {
    let mut writer = SettingsWriter::new("index.json".into());
    submit(&mut writer, book("A", 1));
    let first = writer.take_job().unwrap();
    let early = writer.flush_book(&PathBuf::from("A"));
    submit(&mut writer, book("A", 2));
    failure(&mut writer, first);
    assert_eq!(writer.flush_status(&early), FlushStatus::Pending);
    let second = writer.take_job().unwrap();
    assert_eq!(second.notebook().unwrap().settings.date_format, "A-2");
    failure(&mut writer, second);
    assert!(matches!(
        writer.flush_status(&early),
        FlushStatus::Failed(_)
    ));
    for _ in 0..10 {
        assert!(
            writer.take_job().is_none(),
            "a failed latest snapshot must not spin"
        );
    }
    assert!(writer.retry(&early));
    let retry = writer.take_job().unwrap();
    assert_eq!(retry.notebook().unwrap().settings.date_format, "A-2");
    success(&mut writer, retry);
    assert_eq!(writer.flush_status(&early), FlushStatus::Succeeded);
    let index = writer.take_job().unwrap();
    success(&mut writer, index);
    assert_eq!(writer.retained_snapshot_count(), 0);
}

#[test]
fn an_unrelated_book_and_latest_index_proceed_past_an_older_book_failure() {
    let mut writer = SettingsWriter::new("index.json".into());
    submit(&mut writer, book("A", 1));
    let first = writer.take_job().unwrap();
    submit(&mut writer, book("B", 2));
    let barrier = writer.flush_all();
    failure(&mut writer, first);
    assert_eq!(writer.flush_status(&barrier), FlushStatus::Pending);
    let next = writer.take_job().unwrap();
    assert_eq!(next.notebook().unwrap().root, PathBuf::from("B"));
    success(&mut writer, next);
    let index = writer.take_job().unwrap();
    assert_eq!(index.index().unwrap().active, Some(PathBuf::from("B")));
    success(&mut writer, index);
    let FlushStatus::Failed(errors) = writer.flush_status(&barrier) else {
        panic!("failed A must remain visible after B and its index finish");
    };
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].path, settings_path(&PathBuf::from("A")));
    assert_eq!(
        writer.flush_status(&writer.flush_book(&PathBuf::from("B"))),
        FlushStatus::Succeeded
    );
    assert!(writer.retry(&barrier));
    let retry = writer.take_job().unwrap();
    assert_eq!(retry.notebook().unwrap().root, PathBuf::from("A"));
    success(&mut writer, retry);
    assert!(
        writer.take_job().is_none(),
        "late A must not carry an obsolete index"
    );
    assert_eq!(writer.flush_status(&barrier), FlushStatus::Succeeded);
}

#[test]
fn a_failed_dependency_is_reported_once_and_index_only_edits_drop_it() {
    let mut writer = SettingsWriter::new("index.json".into());
    submit(&mut writer, book("A", 1));
    let job = writer.take_job().unwrap();
    failure(&mut writer, job);
    let barrier = writer.flush_all();
    let FlushStatus::Failed(errors) = writer.flush_status(&barrier) else {
        panic!("blocked index must report its failed book dependency");
    };
    assert_eq!(errors.len(), 1);
    writer.submit(Index::default(), None);
    let index = writer.take_job().unwrap();
    assert!(index.index().unwrap().active.is_none());
    success(&mut writer, index);
    assert!(matches!(
        writer.flush_status(&barrier),
        FlushStatus::Failed(_)
    ));
    assert_eq!(writer.retained_snapshot_count(), 1);
}

#[test]
fn a_new_edit_reactivates_failure_and_an_older_successful_barrier_stays_satisfied() {
    let mut writer = SettingsWriter::new("index.json".into());
    submit(&mut writer, book("A", 1));
    let barrier = writer.flush_book(&PathBuf::from("A"));
    let first = writer.take_job().unwrap();
    failure(&mut writer, first);
    submit(&mut writer, book("A", 2));
    assert_eq!(writer.flush_status(&barrier), FlushStatus::Pending);
    let second = writer.take_job().unwrap();
    success(&mut writer, second);
    assert_eq!(writer.flush_status(&barrier), FlushStatus::Succeeded);
    submit(&mut writer, book("A", 3));
    let third = writer.take_job().unwrap();
    failure(&mut writer, third);
    assert_eq!(writer.flush_status(&barrier), FlushStatus::Succeeded);
    assert!(
        !writer.retry(&barrier),
        "an old barrier must not retry later unrelated edits"
    );
}

#[test]
fn an_index_failure_does_not_block_a_new_book_and_retries_the_latest_index() {
    let mut writer = SettingsWriter::new("index.json".into());
    submit(&mut writer, book("A", 1));
    let first = writer.take_job().unwrap();
    success(&mut writer, first);
    let old_index = writer.take_job().unwrap();
    let old_barrier = writer.flush_all();
    submit(&mut writer, book("B", 2));
    failure(&mut writer, old_index);
    assert_eq!(writer.flush_status(&old_barrier), FlushStatus::Pending);
    let second = writer.take_job().unwrap();
    assert_eq!(second.notebook().unwrap().root, PathBuf::from("B"));
    success(&mut writer, second);
    let latest_index = writer.take_job().unwrap();
    assert_eq!(
        latest_index.index().unwrap().active,
        Some(PathBuf::from("B"))
    );
    failure(&mut writer, latest_index);
    let barrier = writer.flush_all();
    assert!(matches!(
        writer.flush_status(&barrier),
        FlushStatus::Failed(_)
    ));
    assert!(writer.take_job().is_none());
    assert!(writer.retry(&barrier));
    let retry = writer.take_job().unwrap();
    assert_eq!(retry.index().unwrap().active, Some(PathBuf::from("B")));
    success(&mut writer, retry);
    assert_eq!(writer.flush_status(&old_barrier), FlushStatus::Succeeded);
    assert_eq!(writer.flush_status(&barrier), FlushStatus::Succeeded);
}

#[test]
fn coalesced_jobs_write_both_books_and_the_final_active_index_to_disk() {
    let fixture = tempfile::tempdir().unwrap();
    let mut first = Notebook::open(&fixture.path().join("A"), true).unwrap();
    let mut second = Notebook::open(&fixture.path().join("B"), true).unwrap();
    let index_path = fixture.path().join("index.json");
    let mut writer = SettingsWriter::new(index_path.clone());
    submit(&mut writer, first.clone());
    let running = writer.take_job().unwrap();
    for edit in 1..=1_000 {
        first.settings.date_format = format!("A-{edit}");
        second.settings.date_format = format!("B-{edit}");
        submit(&mut writer, first.clone());
        submit(&mut writer, second.clone());
    }
    let result = running.write();
    assert!(writer.complete(running.id(), result));
    let barrier = writer.flush_all();
    let mut remaining_writes = 0;
    while let Some(job) = writer.take_job() {
        let result = job.write();
        assert!(result.is_ok());
        assert!(writer.complete(job.id(), result));
        remaining_writes += 1;
    }
    assert_eq!(remaining_writes, 3);
    assert_eq!(writer.flush_status(&barrier), FlushStatus::Succeeded);
    assert_eq!(
        Notebook::open(&first.root, false)
            .unwrap()
            .settings
            .date_format,
        "A-1000"
    );
    assert_eq!(
        Notebook::open(&second.root, false)
            .unwrap()
            .settings
            .date_format,
        "B-1000"
    );
    assert_eq!(load_index(&index_path).unwrap().active, Some(second.root));
}

#[test]
fn independent_writes_preserve_corrupt_or_unsupported_settings_and_other_destinations() {
    let fixture = tempfile::tempdir().unwrap();
    let book = Notebook::open(&fixture.path().join("A"), true).unwrap();
    let book_path = settings_path(&book.root);
    let index_path = fixture.path().join("index.json");
    let index = index_for(&book);
    for text in [
        "corrupt book",
        r#"{"version":999,"last_file":null,"expanded":[]}"#,
    ] {
        fs::write(&book_path, text).unwrap();
        assert!(save_book(&book).is_err());
        assert_eq!(fs::read_to_string(&book_path).unwrap(), text);
        save_index(&index_path, &index).unwrap();
    }
    fs::write(&book_path, serde_json::to_vec(&book.settings).unwrap()).unwrap();
    for text in [
        "corrupt index",
        r#"{"version":999,"notebooks":[],"active":null}"#,
    ] {
        fs::write(&index_path, text).unwrap();
        assert!(save_index(&index_path, &index).is_err());
        assert_eq!(fs::read_to_string(&index_path).unwrap(), text);
        save_book(&book).unwrap();
    }
}
