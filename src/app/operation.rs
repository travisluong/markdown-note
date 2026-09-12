//! Foreground ownership shared by prompts, navigation, and foreground saves.
//! Background work must release only the operation token it acquired.

/// Identifies one operation within its owning foreground gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OperationId(u64);

/// Allows one foreground operation at a time without an independently settable
/// busy flag. Keep the same gate for the session's lifetime, including document
/// changes, so callbacks cannot reuse an earlier operation's identity.
#[derive(Debug, Default)]
pub(crate) struct ForegroundOperation {
    last_id: u64,
    active: Option<OperationId>,
}

impl ForegroundOperation {
    pub(crate) fn is_busy(&self) -> bool {
        self.active.is_some()
    }

    /// Acquire an idle gate. Exhausting the identity space also returns `None`;
    /// identities never wrap around or become valid again.
    pub(crate) fn begin(&mut self) -> Option<OperationId> {
        if self.is_busy() {
            return None;
        }
        let next_id = self.last_id.checked_add(1)?;
        let operation = OperationId(next_id);
        self.last_id = next_id;
        self.active = Some(operation);
        Some(operation)
    }

    pub(crate) fn owns(&self, operation: OperationId) -> bool {
        self.active == Some(operation)
    }

    /// Release the matching owner; stale and duplicate completions do nothing.
    pub(crate) fn finish(&mut self, operation: OperationId) -> bool {
        if !self.owns(operation) {
            return false;
        }
        self.active = None;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_begin_keeps_the_current_owner() {
        let mut gate = ForegroundOperation::default();
        assert!(!gate.is_busy());
        let current = gate.begin().unwrap();
        assert!(gate.is_busy());
        assert!(gate.owns(current));
        assert_eq!(gate.begin(), None);
        assert!(gate.owns(current));
        assert!(gate.finish(current));
        assert!(!gate.is_busy());
        assert!(!gate.owns(current));
    }

    #[test]
    fn stale_and_duplicate_completions_cannot_release_a_new_owner() {
        let mut gate = ForegroundOperation::default();
        let old = gate.begin().unwrap();
        assert!(gate.finish(old));
        assert!(!gate.finish(old));

        let current = gate.begin().unwrap();
        assert!(!gate.owns(old));
        assert!(!gate.finish(old));
        assert!(gate.is_busy());
        assert!(gate.owns(current));
        assert!(gate.finish(current));
        assert!(!gate.finish(current));
        assert!(!gate.is_busy());
    }

    #[test]
    fn renewal_assigns_monotonically_increasing_identities() {
        let mut gate = ForegroundOperation::default();
        let first = gate.begin().unwrap();
        assert!(gate.finish(first));
        let second = gate.begin().unwrap();
        assert!(second.0 > first.0);
        assert!(gate.finish(second));
        let third = gate.begin().unwrap();
        assert!(third.0 > second.0);
        assert!(gate.owns(third));
    }

    #[test]
    fn exhausted_identities_never_wrap_or_reactivate_an_old_owner() {
        let mut gate = ForegroundOperation {
            last_id: u64::MAX - 1,
            ..ForegroundOperation::default()
        };
        let last = gate.begin().unwrap();
        assert!(gate.finish(last));
        assert_eq!(gate.begin(), None);
        assert!(!gate.is_busy());
        assert!(!gate.owns(last));
        assert!(!gate.finish(last));
    }
}
