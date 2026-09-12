//! Bounded settings write scheduling, independent of executors and window state.
//!
//! A caller runs at most one returned job and reports its result. Each destination
//! retains only its latest snapshot; failed snapshots require retry or a new edit.
//! Notebook roots must be the canonical identities returned by `Notebook::open`.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use super::{
    settings::Index,
    storage::{Notebook, save_book, save_index, settings_path},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Revision(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteJobId(u64);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Destination {
    Book(PathBuf),
    Index,
}

#[derive(Clone, Debug)]
struct Requirement {
    destination: Destination,
    revision: Revision,
}

/// A fixed set of revision requirements belonging to the writer that made it.
#[derive(Clone, Debug, Default)]
pub struct FlushBarrier {
    requirements: Vec<Requirement>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteFailure {
    pub path: PathBuf,
    pub revision: Revision,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlushStatus {
    Pending,
    Succeeded,
    Failed(Vec<WriteFailure>),
}

#[derive(Debug)]
struct Slot<T> {
    submitted: Revision,
    succeeded: Revision,
    snapshot: Option<Arc<T>>,
    failure: Option<String>,
}

impl<T> Default for Slot<T> {
    fn default() -> Self {
        Self {
            submitted: Revision::default(),
            succeeded: Revision::default(),
            snapshot: None,
            failure: None,
        }
    }
}

impl<T> Slot<T> {
    fn submit(&mut self, revision: Revision, value: T) {
        self.submitted = revision;
        self.snapshot = Some(Arc::new(value));
        self.failure = None;
    }

    fn complete(&mut self, revision: Revision, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.succeeded = self.succeeded.max(revision);
                if self.submitted == revision {
                    self.snapshot = None;
                    self.failure = None;
                }
            }
            Err(message) if self.submitted == revision => self.failure = Some(message),
            Err(_) => {} // A newer snapshot already supersedes this failed attempt.
        }
    }
}

#[derive(Debug)]
struct PendingIndex {
    value: Index,
    after_book: Option<(PathBuf, Revision)>,
}

#[derive(Debug)]
enum WriteValue {
    Book(Arc<Notebook>),
    Index(Arc<PendingIndex>),
}

/// An immutable captured write. Completion must report this job's exact ID.
#[derive(Debug)]
pub struct WriteJob {
    id: WriteJobId,
    revision: Revision,
    path: PathBuf,
    value: WriteValue,
}

impl WriteJob {
    pub fn id(&self) -> WriteJobId {
        self.id
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn notebook(&self) -> Option<&Notebook> {
        match &self.value {
            WriteValue::Book(book) => Some(book),
            WriteValue::Index(_) => None,
        }
    }

    pub fn index(&self) -> Option<&Index> {
        match &self.value {
            WriteValue::Book(_) => None,
            WriteValue::Index(index) => Some(&index.value),
        }
    }

    /// Perform the guarded filesystem replacement on the caller's worker.
    pub fn write(&self) -> io::Result<()> {
        match &self.value {
            WriteValue::Book(book) => save_book(book),
            WriteValue::Index(index) => save_index(&self.path, &index.value),
        }
    }
}

#[derive(Debug)]
struct Running {
    id: WriteJobId,
    requirement: Requirement,
}

enum Progress {
    Succeeded,
    Pending,
    Failed(Destination, WriteFailure),
}

/// One writer for an index destination and all notebooks registered through it.
/// Queue order is the order destinations became dirty, not the order of edits.
#[derive(Debug)]
pub struct SettingsWriter {
    index_path: PathBuf,
    books: BTreeMap<PathBuf, Slot<Notebook>>,
    index: Slot<PendingIndex>,
    queue: VecDeque<Destination>,
    running: Option<Running>,
    revision: Revision,
    next_job_id: u64,
}

impl SettingsWriter {
    pub fn new(index_path: PathBuf) -> Self {
        Self {
            index_path,
            books: BTreeMap::new(),
            index: Slot::default(),
            queue: VecDeque::new(),
            running: None,
            revision: Revision::default(),
            next_job_id: 0,
        }
    }

    /// Replace the latest index and, when present, this notebook's latest settings.
    /// The index depends on this submitted book revision, not on unrelated books.
    pub fn submit(&mut self, index: Index, book: Option<Notebook>) -> Revision {
        self.revision.0 = self
            .revision
            .0
            .checked_add(1)
            .expect("settings revision overflow");
        let revision = self.revision;
        let after_book = book.map(|book| {
            let path = settings_path(&book.root);
            self.books
                .entry(path.clone())
                .or_default()
                .submit(revision, book);
            self.enqueue(Destination::Book(path.clone()));
            (path, revision)
        });
        self.index.submit(
            revision,
            PendingIndex {
                value: index,
                after_book,
            },
        );
        self.enqueue(Destination::Index);
        revision
    }

    fn enqueue(&mut self, destination: Destination) {
        if !self.queue.contains(&destination) {
            self.queue.push_back(destination);
        }
    }

    /// Return one eligible job, or None when running, settled, or awaiting retry.
    /// A blocked index is skipped so an unrelated notebook can still proceed.
    pub fn take_job(&mut self) -> Option<WriteJob> {
        if self.running.is_some() {
            return None;
        }
        for _ in 0..self.queue.len() {
            let destination = self.queue.pop_front().expect("queued destination");
            let (revision, path, value) = match &destination {
                Destination::Book(path) => {
                    let slot = &self.books[path];
                    if slot.failure.is_some() {
                        continue;
                    }
                    let Some(book) = &slot.snapshot else { continue };
                    (slot.submitted, path.clone(), WriteValue::Book(book.clone()))
                }
                Destination::Index => {
                    if self.index.failure.is_some() {
                        continue;
                    }
                    let Some(index) = &self.index.snapshot else {
                        continue;
                    };
                    if index
                        .after_book
                        .as_ref()
                        .is_some_and(|(path, revision)| self.books[path].succeeded < *revision)
                    {
                        self.enqueue(destination);
                        continue;
                    }
                    (
                        self.index.submitted,
                        self.index_path.clone(),
                        WriteValue::Index(index.clone()),
                    )
                }
            };
            self.next_job_id = self
                .next_job_id
                .checked_add(1)
                .expect("settings job ID overflow");
            let id = WriteJobId(self.next_job_id);
            self.running = Some(Running {
                id,
                requirement: Requirement {
                    destination,
                    revision,
                },
            });
            return Some(WriteJob {
                id,
                revision,
                path,
                value,
            });
        }
        None
    }

    /// Apply only the active job's completion; stale or duplicate IDs do nothing.
    pub fn complete(&mut self, id: WriteJobId, result: io::Result<()>) -> bool {
        if !self
            .running
            .as_ref()
            .is_some_and(|running| running.id == id)
        {
            return false;
        }
        let running = self.running.take().expect("matching running job");
        let result = result.map_err(|error| error.to_string());
        match &running.requirement.destination {
            Destination::Book(path) => self
                .books
                .get_mut(path)
                .expect("submitted book")
                .complete(running.requirement.revision, result),
            Destination::Index => self.index.complete(running.requirement.revision, result),
        }
        true
    }

    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// Includes failed and running snapshots; bounded by notebooks plus one index.
    /// A returned job can additionally retain one superseded snapshot until it ends.
    pub fn retained_snapshot_count(&self) -> usize {
        self.books
            .values()
            .filter(|slot| slot.snapshot.is_some())
            .count()
            + usize::from(self.index.snapshot.is_some())
    }

    pub fn queued_destination_count(&self) -> usize {
        self.queue.len()
    }

    /// Capture all submissions so far. Newer successful writes satisfy old requirements.
    pub fn flush_all(&self) -> FlushBarrier {
        let mut requirements = self
            .books
            .iter()
            .map(|(path, slot)| Requirement {
                destination: Destination::Book(path.clone()),
                revision: slot.submitted,
            })
            .collect::<Vec<_>>();
        if self.index.submitted != Revision::default() {
            requirements.push(Requirement {
                destination: Destination::Index,
                revision: self.index.submitted,
            });
        }
        FlushBarrier { requirements }
    }

    /// Capture just the notebook that must reach disk before it can be reopened.
    pub fn flush_book(&self, root: &Path) -> FlushBarrier {
        let path = settings_path(root);
        FlushBarrier {
            requirements: self
                .books
                .get(&path)
                .map(|slot| Requirement {
                    destination: Destination::Book(path),
                    revision: slot.submitted,
                })
                .into_iter()
                .collect(),
        }
    }

    fn progress(&self, requirement: &Requirement) -> Progress {
        let (succeeded, failure, submitted, path) = match &requirement.destination {
            Destination::Book(path) => {
                let slot = &self.books[path];
                (slot.succeeded, &slot.failure, slot.submitted, path)
            }
            Destination::Index => (
                self.index.succeeded,
                &self.index.failure,
                self.index.submitted,
                &self.index_path,
            ),
        };
        if succeeded >= requirement.revision {
            return Progress::Succeeded;
        }
        if let Some(message) = failure {
            return Progress::Failed(
                requirement.destination.clone(),
                WriteFailure {
                    path: path.clone(),
                    revision: submitted,
                    message: message.clone(),
                },
            );
        }
        if requirement.destination == Destination::Index
            && let Some((path, revision)) = self
                .index
                .snapshot
                .as_ref()
                .and_then(|index| index.after_book.as_ref())
            && let Progress::Failed(destination, failure) = self.progress(&Requirement {
                destination: Destination::Book(path.clone()),
                revision: *revision,
            })
        {
            return Progress::Failed(destination, failure);
        }
        Progress::Pending
    }

    /// Wait for runnable requirements before reporting terminal failures. A failed
    /// dependency counts as terminal, so a blocked index does not wait forever.
    pub fn flush_status(&self, barrier: &FlushBarrier) -> FlushStatus {
        let mut failures = BTreeMap::new();
        let mut pending = false;
        for requirement in &barrier.requirements {
            match self.progress(requirement) {
                Progress::Succeeded => {}
                Progress::Pending => pending = true,
                Progress::Failed(_, failure) => {
                    failures.insert((failure.path.clone(), failure.revision), failure);
                }
            }
        }
        if pending {
            FlushStatus::Pending
        } else if failures.is_empty() {
            FlushStatus::Succeeded
        } else {
            FlushStatus::Failed(failures.into_values().collect())
        }
    }

    /// Reactivate the latest failed snapshots needed by this barrier, once each.
    /// This never rewinds to the snapshot originally captured by the barrier.
    pub fn retry(&mut self, barrier: &FlushBarrier) -> bool {
        let failed = barrier
            .requirements
            .iter()
            .filter_map(|requirement| match self.progress(requirement) {
                Progress::Failed(destination, _) => Some(destination),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let retried = !failed.is_empty();
        for destination in failed {
            match &destination {
                Destination::Book(path) => {
                    self.books.get_mut(path).expect("submitted book").failure = None
                }
                Destination::Index => self.index.failure = None,
            }
            self.enqueue(destination);
        }
        retried
    }
}
