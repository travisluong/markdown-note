//! Session-only file visits. A traversal is a proposal until installation succeeds.
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
pub(crate) enum Direction {
    Back,
    Forward,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Visit {
    revision: u64,
    index: usize,
    pub(crate) path: PathBuf,
}

#[derive(Default, Debug, PartialEq, Eq)]
pub(crate) struct NavigationHistory {
    entries: Vec<PathBuf>,
    cursor: Option<usize>,
    revision: u64,
}

impl NavigationHistory {
    pub(crate) fn target(&self, direction: Direction) -> Option<Visit> {
        let cursor = self.cursor?;
        let index = match direction {
            Direction::Back => cursor.checked_sub(1)?,
            Direction::Forward => cursor.checked_add(1)?,
        };
        Some(Visit {
            revision: self.revision,
            index,
            path: self.entries.get(index)?.clone(),
        })
    }

    pub(crate) fn accepts(&self, visit: &Visit) -> bool {
        visit.revision == self.revision && self.entries.get(visit.index) == Some(&visit.path)
    }

    pub(crate) fn commit(&mut self, visit: &Visit, installed: &Path) -> bool {
        if !self.accepts(visit) || visit.path != installed {
            return false;
        }
        self.cursor = Some(visit.index);
        self.revision = self
            .revision
            .checked_add(1)
            .expect("navigation revision exhausted");
        true
    }

    pub(crate) fn record(&mut self, path: PathBuf) {
        if self.cursor.and_then(|cursor| self.entries.get(cursor)) == Some(&path) {
            return;
        }
        self.entries
            .truncate(self.cursor.map_or(0, |cursor| cursor + 1));
        self.entries.push(path);
        self.cursor = Some(self.entries.len() - 1);
        self.revision = self
            .revision
            .checked_add(1)
            .expect("navigation revision exhausted");
    }

    pub(crate) fn remap(&mut self, from: &Path, to: &Path) {
        for path in &mut self.entries {
            if let Ok(relative) = path.strip_prefix(from) {
                *path = to.join(relative);
            }
        }
        self.revision = self
            .revision
            .checked_add(1)
            .expect("navigation revision exhausted");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundaries_proposals_branching_and_stale_tickets() {
        let mut history = NavigationHistory::default();
        assert!(history.target(Direction::Back).is_none());
        for path in ["a", "a", "b", "c"] {
            history.record(path.into());
        }
        assert_eq!(history.entries.len(), 3);
        assert!(history.target(Direction::Forward).is_none());
        let back = history.target(Direction::Back).unwrap();
        assert_eq!(history.cursor, Some(2)); // Cancellation/failure needs no mutation.
        assert!(!history.commit(&back, Path::new("wrong")));
        assert!(history.commit(&back, Path::new("b")));
        assert!(!history.commit(&back, Path::new("b")));
        assert_eq!(
            history.target(Direction::Forward).unwrap().path,
            Path::new("c")
        );
        history.record("d".into());
        assert_eq!(
            history.entries,
            [PathBuf::from("a"), "b".into(), "d".into()]
        );
        assert!(history.target(Direction::Forward).is_none());
    }

    #[test]
    fn moves_remap_files_and_descendants_and_invalidate_proposals() {
        let mut history = NavigationHistory::default();
        for path in ["root/a.md", "root/folder/b.md", "other/c.md"] {
            history.record(path.into());
        }
        let stale = history.target(Direction::Back).unwrap();
        history.remap(Path::new("root/folder"), Path::new("root/renamed"));
        assert!(!history.accepts(&stale));
        assert_eq!(
            history.target(Direction::Back).unwrap().path,
            Path::new("root/renamed/b.md")
        );
        history.remap(Path::new("root/a.md"), Path::new("root/z.md"));
        assert_eq!(history.entries[0], Path::new("root/z.md"));
    }
}
