//! Pure ownership and transitions for one document session.
//!
//! The adapter owns tasks and captures immutable editor snapshots when `Write`
//! is emitted. This model owns the active content and operation identities; it
//! never imports an editor, copies source text, or treats edit revision as dirty.
//! Apply effects in order and notify dependent views on `StateChanged`.

/// Identities are session-local and never reused, including after installation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocumentId(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationId(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteId(u64);

/// The only active content. `D` owns document metadata and its saved baseline;
/// the actual current editor buffer remains in the adapter.
#[derive(Debug, PartialEq, Eq)]
pub enum Content<D, I> {
    Empty,
    Document { id: DocumentId, value: D },
    Image { id: DocumentId, value: I },
}

impl<D, I> Content<D, I> {
    pub fn identity(&self) -> Option<DocumentId> {
        match self {
            Self::Empty => None,
            Self::Document { id, .. } | Self::Image { id, .. } => Some(*id),
        }
    }

    pub fn document_id(&self) -> Option<DocumentId> {
        match self {
            Self::Document { id, .. } => Some(*id),
            _ => None,
        }
    }
}

/// An owned load result; installation assigns its identity.
pub enum Loaded<D, I> {
    Empty,
    Document(D),
    Image(I),
}

/// Mutable values are available only inside an accepted move completion.
/// Identity and content kind cannot be changed by the callback.
pub enum MovingContent<'a, D, I> {
    Empty,
    Document(&'a mut D),
    Image(&'a mut I),
}

/// A filesystem move can succeed while revalidating its new disk baseline
/// fails. Keep the new path/identity in that case and pause automatic writes.
pub struct MoveOutcome {
    pub dirty: bool,
    pub conflict: bool,
}

/// A current, synchronous comparison with the active document's saved baseline.
/// An Undo may supply `dirty: false` even after many intervening edit events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Observed {
    pub document: Option<DocumentId>,
    pub dirty: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteTicket {
    pub id: WriteId,
    pub document: DocumentId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutosaveTicket {
    serial: u64,
    pub document: DocumentId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Save,
    Discard,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Pick,
    Load,
    Close,
    ActivateNotebook,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhaseKind {
    Restoring,
    WaitingForWrite,
    Prompt,
    Writing,
    Dispatching,
    Picking,
    Loading,
    ResolvingActivation,
    FlushingActivation,
    ActivationFlushFailed,
    LoadingNotebook,
    Moving,
    EntryOperation,
    Closing,
    CloseFailed,
    Error,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    Busy,
    NoDocument,
    IdentityExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveFailure {
    Io,
    Conflict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RetryPause {
    None,
    UntilEdit,
    Conflict,
}

/// Effects contain identities and application actions, never document buffers.
#[derive(Debug, PartialEq, Eq)]
pub enum Effect<A> {
    StateChanged,
    CancelAutosave,
    ArmAutosave(AutosaveTicket),
    Prompt(OperationId),
    Write(WriteTicket),
    BaselineAccepted(WriteTicket),
    WriteFailed(WriteTicket),
    Continue {
        operation: OperationId,
        action: A,
    },
    Pick(OperationId),
    Load(OperationId),
    Move(OperationId),
    RestoreIndex(OperationId),
    RestoreFailed(OperationId),
    ResolveActivation(OperationId),
    FlushActivation {
        operation: OperationId,
        retry: bool,
    },
    ActivationFlushFailed(OperationId),
    ActivationFailed(OperationId),
    FlushClose {
        operation: OperationId,
        discard_failed: bool,
        retry: bool,
    },
    Close,
    CloseFailed(OperationId),
    ShowError(OperationId),
    MoveFailed(OperationId),
    Cancelled {
        operation: OperationId,
        /// Present while the model still owns an undispatched action. Stage
        /// adapters retain their own load/picker context after `Continue`.
        action: Option<A>,
    },
    Installed {
        previous: Option<DocumentId>,
        current: Option<DocumentId>,
        discarded_document: Option<DocumentId>,
    },
    Moved(Option<DocumentId>),
    MoveBaselineConflict(DocumentId),
    Rejected(Rejection),
}

enum Waiting<A> {
    Request(A, bool),
    Save(Option<A>),
}

enum LoadTarget<A> {
    Current,
    Next(A),
    Retain,
}

enum Phase<A> {
    Idle,
    Restoring(OperationId),
    Waiting {
        id: OperationId,
        write: WriteTicket,
        next: Waiting<A>,
    },
    Prompt {
        id: OperationId,
        next: A,
    },
    Writing {
        id: OperationId,
        write: WriteTicket,
        next: Option<A>,
    },
    Dispatching(OperationId),
    Picking {
        id: OperationId,
        approved: bool,
    },
    Loading {
        id: OperationId,
        target: LoadTarget<A>,
        notebook: bool,
    },
    ResolvingActivation(OperationId),
    FlushingActivation(OperationId),
    ActivationFlushFailed(OperationId),
    Moving(OperationId),
    EntryOperation(OperationId),
    Closing {
        id: OperationId,
        discard_failed: bool,
    },
    CloseFailed(OperationId),
    Error(OperationId),
    Closed(OperationId),
}

impl<A> Phase<A> {
    fn status(&self) -> Option<(OperationId, PhaseKind)> {
        Some(match self {
            Self::Idle => return None,
            Self::Restoring(id) => (*id, PhaseKind::Restoring),
            Self::Waiting { id, .. } => (*id, PhaseKind::WaitingForWrite),
            Self::Prompt { id, .. } => (*id, PhaseKind::Prompt),
            Self::Writing { id, .. } => (*id, PhaseKind::Writing),
            Self::Dispatching(id) => (*id, PhaseKind::Dispatching),
            Self::Picking { id, .. } => (*id, PhaseKind::Picking),
            Self::Loading { id, notebook, .. } => (
                *id,
                if *notebook {
                    PhaseKind::LoadingNotebook
                } else {
                    PhaseKind::Loading
                },
            ),
            Self::ResolvingActivation(id) => (*id, PhaseKind::ResolvingActivation),
            Self::FlushingActivation(id) => (*id, PhaseKind::FlushingActivation),
            Self::ActivationFlushFailed(id) => (*id, PhaseKind::ActivationFlushFailed),
            Self::Moving(id) => (*id, PhaseKind::Moving),
            Self::EntryOperation(id) => (*id, PhaseKind::EntryOperation),
            Self::Closing { id, .. } => (*id, PhaseKind::Closing),
            Self::CloseFailed(id) => (*id, PhaseKind::CloseFailed),
            Self::Error(id) => (*id, PhaseKind::Error),
            Self::Closed(id) => (*id, PhaseKind::Closed),
        })
    }
}

pub struct Session<D, I, A> {
    content: Content<D, I>,
    phase: Phase<A>,
    write: Option<WriteTicket>,
    autosave: Option<AutosaveTicket>,
    autosave_enabled: bool,
    retry_pause: RetryPause,
    last_document: u64,
    last_operation: u64,
    last_write: u64,
    last_autosave: u64,
}

impl<D, I, A> Default for Session<D, I, A> {
    fn default() -> Self {
        Self {
            content: Content::Empty,
            phase: Phase::Idle,
            write: None,
            autosave: None,
            autosave_enabled: false,
            retry_pause: RetryPause::None,
            last_document: 0,
            last_operation: 0,
            last_write: 0,
            last_autosave: 0,
        }
    }
}

impl<D, I, A> Session<D, I, A> {
    pub fn content(&self) -> &Content<D, I> {
        &self.content
    }

    pub fn foreground(&self) -> Option<(OperationId, PhaseKind)> {
        self.phase.status()
    }

    pub fn write(&self) -> Option<WriteTicket> {
        self.write
    }

    pub fn autosave(&self) -> Option<AutosaveTicket> {
        self.autosave
    }

    pub fn is_busy(&self) -> bool {
        self.foreground().is_some()
    }

    pub fn save_conflicted(&self) -> bool {
        self.retry_pause == RetryPause::Conflict
    }

    fn matches(&self, observed: Observed) -> bool {
        self.content.document_id() == observed.document
            && (!observed.dirty || observed.document.is_some())
    }

    fn operation(&mut self) -> Option<OperationId> {
        self.last_operation = self.last_operation.checked_add(1)?;
        Some(OperationId(self.last_operation))
    }

    fn owns(&self, operation: OperationId, kind: PhaseKind) -> bool {
        self.foreground() == Some((operation, kind))
    }

    fn changed(mut effects: Vec<Effect<A>>) -> Vec<Effect<A>> {
        if !matches!(effects.last(), Some(Effect::StateChanged)) {
            effects.push(Effect::StateChanged);
        }
        effects
    }

    fn cancel_timer(&mut self, effects: &mut Vec<Effect<A>>) {
        if self.autosave.take().is_some() {
            effects.push(Effect::CancelAutosave);
        }
    }

    fn resume(&mut self, observed: Observed, effects: &mut Vec<Effect<A>>) {
        if !self.matches(observed) || !observed.dirty {
            return;
        }
        self.arm_autosave(observed.document.expect("dirty document"), effects);
    }

    fn arm_autosave(&mut self, document: DocumentId, effects: &mut Vec<Effect<A>>) {
        if self.content.document_id() != Some(document)
            || !self.autosave_enabled
            || self.retry_pause != RetryPause::None
            || self.is_busy()
            || self.write.is_some()
        {
            return;
        }
        let Some(serial) = self.last_autosave.checked_add(1) else {
            effects.push(Effect::Rejected(Rejection::IdentityExhausted));
            return;
        };
        self.cancel_timer(effects);
        self.last_autosave = serial;
        let ticket = AutosaveTicket { serial, document };
        self.autosave = Some(ticket);
        effects.push(Effect::ArmAutosave(ticket));
    }

    pub fn set_autosave(&mut self, enabled: bool, observed: Observed) -> Vec<Effect<A>> {
        if !self.matches(observed) {
            return vec![];
        }
        self.autosave_enabled = enabled;
        let mut effects = vec![];
        self.cancel_timer(&mut effects);
        self.resume(observed, &mut effects);
        Self::changed(effects)
    }

    /// Arm debounce for a real edit event, including Undo/Redo, without comparing
    /// source text on the UI thread. `autosave_due` requires the actual dirty
    /// fact; an Undo back to the saved snapshot therefore performs no write.
    /// Only ordinary I/O pauses clear on edit; a disk conflict stays paused.
    pub fn edit_notice(&mut self, document: DocumentId) -> Vec<Effect<A>> {
        if self.content.document_id() != Some(document) {
            return vec![];
        }
        if self.retry_pause == RetryPause::UntilEdit {
            self.retry_pause = RetryPause::None;
        }
        let mut effects = vec![];
        self.cancel_timer(&mut effects);
        self.arm_autosave(document, &mut effects);
        Self::changed(effects)
    }

    pub fn request(&mut self, action: A, observed: Observed) -> Vec<Effect<A>> {
        self.request_policy(action, observed, true)
    }
    /// Entry operations outside the active content wait for writes but retain its draft.
    pub fn request_retaining(&mut self, action: A, observed: Observed) -> Vec<Effect<A>> {
        self.request_policy(action, observed, false)
    }
    fn request_policy(
        &mut self,
        action: A,
        observed: Observed,
        prompt_dirty: bool,
    ) -> Vec<Effect<A>> {
        if !self.matches(observed) {
            return vec![];
        }
        if self.is_busy() {
            return vec![Effect::Rejected(Rejection::Busy)];
        }
        let Some(operation) = self.operation() else {
            return vec![Effect::Rejected(Rejection::IdentityExhausted)];
        };
        let mut effects = vec![];
        self.cancel_timer(&mut effects);
        if let Some(write) = self.write {
            self.phase = Phase::Waiting {
                id: operation,
                write,
                next: Waiting::Request(action, prompt_dirty),
            };
        } else if observed.dirty && prompt_dirty {
            self.phase = Phase::Prompt {
                id: operation,
                next: action,
            };
            effects.push(Effect::Prompt(operation));
        } else {
            self.phase = Phase::Dispatching(operation);
            effects.push(Effect::Continue { operation, action });
        }
        Self::changed(effects)
    }

    /// Manual Save always writes, including a clean document, so the storage
    /// adapter still performs its external-change/conflict checks.
    pub fn save(&mut self, next: Option<A>) -> Vec<Effect<A>> {
        if self.is_busy() {
            return vec![Effect::Rejected(Rejection::Busy)];
        }
        if self.content.document_id().is_none() {
            return vec![Effect::Rejected(Rejection::NoDocument)];
        }
        let Some(operation) = self.operation() else {
            return vec![Effect::Rejected(Rejection::IdentityExhausted)];
        };
        let mut effects = vec![];
        self.cancel_timer(&mut effects);
        if let Some(write) = self.write {
            self.phase = Phase::Waiting {
                id: operation,
                write,
                next: Waiting::Save(next),
            };
        } else {
            self.start_write(Some((operation, next)), &mut effects);
        }
        Self::changed(effects)
    }

    fn start_write(
        &mut self,
        foreground: Option<(OperationId, Option<A>)>,
        effects: &mut Vec<Effect<A>>,
    ) {
        let Some(document) = self.content.document_id() else {
            return;
        };
        let Some(serial) = self.last_write.checked_add(1) else {
            effects.push(Effect::Rejected(Rejection::IdentityExhausted));
            return;
        };
        debug_assert!(self.write.is_none());
        self.last_write = serial;
        let write = WriteTicket {
            id: WriteId(serial),
            document,
        };
        self.write = Some(write);
        if let Some((id, next)) = foreground {
            self.phase = Phase::Writing { id, write, next };
        }
        effects.push(Effect::Write(write));
    }

    pub fn autosave_due(&mut self, ticket: AutosaveTicket, observed: Observed) -> Vec<Effect<A>> {
        if self.autosave != Some(ticket) || !self.matches(observed) {
            return vec![];
        }
        self.autosave = None;
        let mut effects = vec![];
        if observed.dirty
            && self.autosave_enabled
            && self.retry_pause == RetryPause::None
            && !self.is_busy()
            && self.write.is_none()
        {
            self.start_write(None, &mut effects);
        }
        Self::changed(effects)
    }

    pub fn answer(
        &mut self,
        operation: OperationId,
        choice: Choice,
        observed: Observed,
    ) -> Vec<Effect<A>> {
        if !self.owns(operation, PhaseKind::Prompt) || !self.matches(observed) {
            return vec![];
        }
        if choice == Choice::Cancel {
            return self.cancel(operation, observed);
        }
        let Phase::Prompt { next, .. } = std::mem::replace(&mut self.phase, Phase::Idle) else {
            unreachable!()
        };
        let effects = if choice == Choice::Save {
            self.save(Some(next))
        } else {
            self.continue_action(next)
        };
        Self::changed(effects)
    }

    fn continue_action(&mut self, action: A) -> Vec<Effect<A>> {
        let Some(operation) = self.operation() else {
            return Self::changed(vec![Effect::Rejected(Rejection::IdentityExhausted)]);
        };
        self.phase = Phase::Dispatching(operation);
        Self::changed(vec![Effect::Continue { operation, action }])
    }

    /// Update the matching saved baseline exactly once. The callback must return
    /// whether current text differs from the captured snapshot it just accepted,
    /// not whether an edit revision changed. Obsolete foreground ownership does
    /// not prevent baseline acceptance or release an unrelated current phase.
    pub fn write_succeeded(
        &mut self,
        ticket: WriteTicket,
        accept_baseline: impl FnOnce(&mut D) -> bool,
    ) -> Vec<Effect<A>> {
        if self.write != Some(ticket) {
            return vec![];
        }
        self.write = None;
        let mut effects = vec![];
        let dirty = match &mut self.content {
            Content::Document { id, value } if *id == ticket.document => {
                let dirty = accept_baseline(value);
                effects.push(Effect::BaselineAccepted(ticket));
                dirty
            }
            _ => return Self::changed(effects),
        };
        self.retry_pause = RetryPause::None;
        self.after_write(
            ticket,
            true,
            Observed {
                document: Some(ticket.document),
                dirty,
            },
            &mut effects,
        );
        Self::changed(effects)
    }

    /// On failure, `observed` compares current text with the unchanged old saved
    /// baseline. The storage error stays in the adapter for `WriteFailed`.
    pub fn write_failed(
        &mut self,
        ticket: WriteTicket,
        failure: SaveFailure,
        observed: Observed,
    ) -> Vec<Effect<A>> {
        if self.write != Some(ticket) || !self.matches(observed) {
            return vec![];
        }
        self.write = None;
        if failure == SaveFailure::Conflict || self.retry_pause == RetryPause::Conflict {
            self.retry_pause = RetryPause::Conflict;
        } else {
            self.retry_pause = RetryPause::UntilEdit;
        }
        let mut effects = vec![Effect::WriteFailed(ticket)];
        self.after_write(ticket, false, observed, &mut effects);
        Self::changed(effects)
    }

    fn after_write(
        &mut self,
        ticket: WriteTicket,
        success: bool,
        observed: Observed,
        effects: &mut Vec<Effect<A>>,
    ) {
        let owns = matches!(&self.phase, Phase::Waiting { write, .. } | Phase::Writing { write, .. } if *write == ticket);
        if owns {
            match std::mem::replace(&mut self.phase, Phase::Idle) {
                Phase::Waiting {
                    next: Waiting::Request(action, prompt_dirty),
                    ..
                } => effects.extend(self.request_policy(action, observed, prompt_dirty)),
                Phase::Waiting {
                    next: Waiting::Save(next),
                    ..
                } => effects.extend(self.save(next)),
                Phase::Writing { id, next, .. } => {
                    if success {
                        if let Some(action) = next {
                            // A late edit must be saved/prompted before navigation.
                            effects.extend(self.request(action, observed));
                        }
                    } else {
                        effects.push(Effect::Cancelled {
                            operation: id,
                            action: next,
                        });
                    }
                }
                _ => unreachable!(),
            }
        }
        if success {
            self.resume(observed, effects);
        }
    }

    /// An auxiliary picker may coexist with an existing background write. Its
    /// selection enters the normal request/save decision after picking.
    pub fn pick(&mut self) -> Vec<Effect<A>> {
        if self.is_busy() {
            return vec![Effect::Rejected(Rejection::Busy)];
        }
        let Some(operation) = self.operation() else {
            return vec![Effect::Rejected(Rejection::IdentityExhausted)];
        };
        let mut effects = vec![];
        self.cancel_timer(&mut effects);
        self.phase = Phase::Picking {
            id: operation,
            approved: false,
        };
        effects.push(Effect::Pick(operation));
        Self::changed(effects)
    }

    /// Startup index restoration requires an empty session.
    /// It owns the same foreground slot as navigation, without a fake load.
    pub fn restore(&mut self) -> Vec<Effect<A>> {
        if self.is_busy() || self.write.is_some() || !matches!(self.content, Content::Empty) {
            return vec![Effect::Rejected(Rejection::Busy)];
        }
        let Some(operation) = self.operation() else {
            return vec![Effect::Rejected(Rejection::IdentityExhausted)];
        };
        self.phase = Phase::Restoring(operation);
        Self::changed(vec![Effect::RestoreIndex(operation)])
    }

    /// A restored active notebook enters approved dispatch. `Ok(None)` leaves
    /// the initially empty session idle; failure preserves the index on disk.
    pub fn restored(
        &mut self,
        operation: OperationId,
        next: Result<Option<A>, ()>,
    ) -> Vec<Effect<A>> {
        if !self.owns(operation, PhaseKind::Restoring) {
            return vec![];
        }
        self.phase = Phase::Idle;
        match next {
            Ok(Some(action)) => self.continue_action(action),
            Ok(None) => Self::changed(vec![]),
            Err(()) => Self::changed(vec![Effect::RestoreFailed(operation)]),
        }
    }

    /// Errors discovered outside a navigation job (for example an invalid
    /// local link) still acquire ownership until their prompt is dismissed.
    pub fn show_error(&mut self) -> Vec<Effect<A>> {
        if self.is_busy() {
            return vec![Effect::Rejected(Rejection::Busy)];
        }
        let Some(operation) = self.operation() else {
            return vec![Effect::Rejected(Rejection::IdentityExhausted)];
        };
        let mut effects = vec![];
        self.cancel_timer(&mut effects);
        self.phase = Phase::Error(operation);
        effects.push(Effect::ShowError(operation));
        Self::changed(effects)
    }

    pub fn dispatch(&mut self, operation: OperationId, stage: Stage) -> Vec<Effect<A>> {
        if !self.owns(operation, PhaseKind::Dispatching) {
            return vec![];
        }
        let effect = match stage {
            Stage::Pick => {
                self.phase = Phase::Picking {
                    id: operation,
                    approved: true,
                };
                Effect::Pick(operation)
            }
            Stage::Load => {
                self.phase = Phase::Loading {
                    id: operation,
                    target: LoadTarget::Current,
                    notebook: false,
                };
                Effect::Load(operation)
            }
            Stage::Close => {
                self.phase = Phase::Closing {
                    id: operation,
                    discard_failed: false,
                };
                Effect::FlushClose {
                    operation,
                    discard_failed: false,
                    retry: false,
                }
            }
            Stage::ActivateNotebook => {
                self.phase = Phase::ResolvingActivation(operation);
                Effect::ResolveActivation(operation)
            }
        };
        Self::changed(vec![effect])
    }

    /// Finish an approved synchronous action that does not replace content.
    pub fn continued(&mut self, operation: OperationId, observed: Observed) -> Vec<Effect<A>> {
        if !self.owns(operation, PhaseKind::Dispatching) || !self.matches(observed) {
            return vec![];
        }
        self.phase = Phase::Idle;
        let mut effects = vec![];
        self.resume(observed, &mut effects);
        Self::changed(effects)
    }

    pub fn picked(
        &mut self,
        operation: OperationId,
        action: Option<A>,
        observed: Observed,
    ) -> Vec<Effect<A>> {
        if !self.owns(operation, PhaseKind::Picking) || !self.matches(observed) {
            return vec![];
        }
        let Some(action) = action else {
            return self.cancel(operation, observed);
        };
        let Phase::Picking { approved, .. } = std::mem::replace(&mut self.phase, Phase::Idle)
        else {
            unreachable!()
        };
        let effects = if approved {
            self.continue_action(action)
        } else {
            self.request(action, observed)
        };
        Self::changed(effects)
    }

    pub fn pick_failed(&mut self, operation: OperationId) -> Vec<Effect<A>> {
        if !self.owns(operation, PhaseKind::Picking) {
            return vec![];
        }
        self.phase = Phase::Error(operation);
        Self::changed(vec![Effect::ShowError(operation)])
    }

    /// Resolve/canonicalize the target before capturing its settings barrier.
    pub fn activation_resolved(
        &mut self,
        operation: OperationId,
        success: bool,
        observed: Observed,
    ) -> Vec<Effect<A>> {
        if !self.owns(operation, PhaseKind::ResolvingActivation) || !self.matches(observed) {
            return vec![];
        }
        if success {
            self.phase = Phase::FlushingActivation(operation);
            Self::changed(vec![Effect::FlushActivation {
                operation,
                retry: false,
            }])
        } else {
            self.phase = Phase::Idle;
            let mut effects = vec![Effect::ActivationFailed(operation)];
            self.resume(observed, &mut effects);
            Self::changed(effects)
        }
    }

    /// A target notebook is read only after all writes captured by its barrier
    /// succeeded. Failure stays owned while the user chooses Retry or Cancel.
    pub fn activation_flushed(&mut self, operation: OperationId, success: bool) -> Vec<Effect<A>> {
        if !self.owns(operation, PhaseKind::FlushingActivation) {
            return vec![];
        }
        if success {
            self.phase = Phase::Loading {
                id: operation,
                target: LoadTarget::Current,
                notebook: true,
            };
            Self::changed(vec![Effect::Load(operation)])
        } else {
            self.phase = Phase::ActivationFlushFailed(operation);
            Self::changed(vec![Effect::ActivationFlushFailed(operation)])
        }
    }

    pub fn retry_activation(&mut self, operation: OperationId) -> Vec<Effect<A>> {
        if !self.owns(operation, PhaseKind::ActivationFlushFailed) {
            return vec![];
        }
        let Some(next) = self.operation() else {
            return vec![Effect::Rejected(Rejection::IdentityExhausted)];
        };
        self.phase = Phase::FlushingActivation(next);
        Self::changed(vec![Effect::FlushActivation {
            operation: next,
            retry: true,
        }])
    }

    /// Keep one in-flight tree load and only its latest target. `None` means a
    /// folder/current note: discard the result and retain the editor.
    pub fn retarget_load(&mut self, operation: OperationId, action: Option<A>) -> Vec<Effect<A>> {
        if !self.owns(operation, PhaseKind::Loading) {
            return vec![];
        }
        if let Phase::Loading { target, .. } = &mut self.phase {
            *target = action.map_or(LoadTarget::Retain, LoadTarget::Next);
        }
        Self::changed(vec![])
    }

    /// Notebook activation can select a retained tab instead of replacing this
    /// session. Only the matching successful notebook load releases its owner.
    pub fn activation_retained(
        &mut self,
        operation: OperationId,
        observed: Observed,
    ) -> Vec<Effect<A>> {
        if !self.owns(operation, PhaseKind::LoadingNotebook) || !self.matches(observed) {
            return vec![];
        }
        self.phase = Phase::Idle;
        let mut effects = vec![];
        self.resume(observed, &mut effects);
        Self::changed(effects)
    }

    /// `Err(())` reports the adapter's retained load error through `ShowError`.
    /// Superseded results, including errors, never replace retained content.
    pub fn loaded(
        &mut self,
        operation: OperationId,
        result: Result<Loaded<D, I>, ()>,
        observed: Observed,
    ) -> Vec<Effect<A>> {
        if !matches!(&self.phase, Phase::Loading { id, .. } if *id == operation)
            || !self.matches(observed)
        {
            return vec![];
        }
        let Phase::Loading {
            target, notebook, ..
        } = std::mem::replace(&mut self.phase, Phase::Idle)
        else {
            unreachable!()
        };
        match target {
            LoadTarget::Next(action) => return self.continue_action(action),
            LoadTarget::Retain => {
                let mut effects = vec![Effect::Cancelled {
                    operation,
                    action: None,
                }];
                self.resume(observed, &mut effects);
                return Self::changed(effects);
            }
            LoadTarget::Current => {}
        }
        let Ok(loaded) = result else {
            if notebook {
                let mut effects = vec![Effect::ActivationFailed(operation)];
                self.resume(observed, &mut effects);
                return Self::changed(effects);
            }
            self.phase = Phase::Error(operation);
            return Self::changed(vec![Effect::ShowError(operation)]);
        };
        // Navigation waits for existing writes before dispatch; moves and
        // installations therefore cannot invalidate an in-flight write path.
        debug_assert!(self.write.is_none());
        let id = if matches!(loaded, Loaded::Empty) {
            None
        } else {
            let Some(serial) = self.last_document.checked_add(1) else {
                let mut effects = vec![Effect::Rejected(Rejection::IdentityExhausted)];
                self.resume(observed, &mut effects);
                return Self::changed(effects);
            };
            self.last_document = serial;
            Some(DocumentId(serial))
        };
        let previous = self.content.identity();
        self.content = match loaded {
            Loaded::Empty => Content::Empty,
            Loaded::Document(value) => Content::Document {
                id: id.expect("document identity"),
                value,
            },
            Loaded::Image(value) => Content::Image {
                id: id.expect("image identity"),
                value,
            },
        };
        self.retry_pause = RetryPause::None;
        let mut effects = vec![];
        self.cancel_timer(&mut effects);
        effects.push(Effect::Installed {
            previous,
            current: id,
            discarded_document: observed.dirty.then_some(observed.document).flatten(),
        });
        Self::changed(effects)
    }

    /// An approved entry operation owns the foreground until its filesystem result.
    pub fn begin_entry_operation(&mut self, operation: OperationId) -> Vec<Effect<A>> {
        if !self.owns(operation, PhaseKind::Dispatching) || self.write.is_some() {
            return vec![];
        }
        self.phase = Phase::EntryOperation(operation);
        Self::changed(vec![])
    }
    pub fn entry_operation_finished(
        &mut self,
        operation: OperationId,
        loaded: Option<Loaded<D, I>>,
        observed: Observed,
    ) -> Vec<Effect<A>> {
        if !self.owns(operation, PhaseKind::EntryOperation) || !self.matches(observed) {
            return vec![];
        }
        if let Some(loaded) = loaded {
            self.phase = Phase::Loading {
                id: operation,
                target: LoadTarget::Current,
                notebook: false,
            };
            return self.loaded(operation, Ok(loaded), observed);
        }
        self.phase = Phase::Idle;
        let mut effects = vec![];
        self.resume(observed, &mut effects);
        Self::changed(effects)
    }
    /// A failed trash preserves the current buffer; only confirmed deletion installs Empty.
    pub fn entry_deleted(
        &mut self,
        operation: OperationId,
        success: bool,
        contains_active: bool,
        observed: Observed,
    ) -> Vec<Effect<A>> {
        self.entry_operation_finished(
            operation,
            (success && contains_active).then_some(Loaded::Empty),
            observed,
        )
    }
    pub fn move_content(&mut self) -> Vec<Effect<A>> {
        if self.is_busy() || self.write.is_some() {
            return vec![Effect::Rejected(Rejection::Busy)];
        }
        let Some(operation) = self.operation() else {
            return vec![Effect::Rejected(Rejection::IdentityExhausted)];
        };
        let mut effects = vec![];
        self.cancel_timer(&mut effects);
        self.phase = Phase::Moving(operation);
        effects.push(Effect::Move(operation));
        Self::changed(effects)
    }

    /// Remap paths/metadata in place only after the matching move succeeds. The
    /// callback reports actual old-baseline dirtiness; identity/history survive.
    pub fn moved(
        &mut self,
        operation: OperationId,
        remap: impl FnOnce(MovingContent<'_, D, I>) -> MoveOutcome,
    ) -> Vec<Effect<A>> {
        if !self.owns(operation, PhaseKind::Moving) {
            return vec![];
        }
        let outcome = remap(match &mut self.content {
            Content::Empty => MovingContent::Empty,
            Content::Document { value, .. } => MovingContent::Document(value),
            Content::Image { value, .. } => MovingContent::Image(value),
        });
        self.phase = Phase::Idle;
        let mut effects = vec![Effect::Moved(self.content.identity())];
        if outcome.conflict
            && let Some(document) = self.content.document_id()
        {
            self.retry_pause = RetryPause::Conflict;
            effects.push(Effect::MoveBaselineConflict(document));
        }
        self.resume(
            Observed {
                document: self.content.document_id(),
                dirty: outcome.dirty,
            },
            &mut effects,
        );
        Self::changed(effects)
    }

    pub fn move_failed(&mut self, operation: OperationId, observed: Observed) -> Vec<Effect<A>> {
        if !self.owns(operation, PhaseKind::Moving) || !self.matches(observed) {
            return vec![];
        }
        self.phase = Phase::Idle;
        let mut effects = vec![Effect::MoveFailed(operation)];
        self.resume(observed, &mut effects);
        Self::changed(effects)
    }

    pub fn close_flushed(&mut self, operation: OperationId, success: bool) -> Vec<Effect<A>> {
        if !self.owns(operation, PhaseKind::Closing) {
            return vec![];
        }
        let discard_failed = matches!(
            self.phase,
            Phase::Closing {
                discard_failed: true,
                ..
            }
        );
        self.phase = if success || discard_failed {
            Phase::Closed(operation)
        } else {
            Phase::CloseFailed(operation)
        };
        Self::changed(vec![if success || discard_failed {
            Effect::Close
        } else {
            Effect::CloseFailed(operation)
        }])
    }

    pub fn retry_close(&mut self, operation: OperationId) -> Vec<Effect<A>> {
        if !self.owns(operation, PhaseKind::CloseFailed) {
            return vec![];
        }
        let Some(next) = self.operation() else {
            return vec![Effect::Rejected(Rejection::IdentityExhausted)];
        };
        self.phase = Phase::Closing {
            id: next,
            discard_failed: false,
        };
        Self::changed(vec![Effect::FlushClose {
            operation: next,
            discard_failed: false,
            retry: true,
        }])
    }

    /// Explicit permission to close after failed preference writes. The adapter
    /// must still wait for pending/late preference jobs; only their failures may
    /// be ignored. A failed close alone never grants this permission.
    pub fn discard_close_preferences(&mut self, operation: OperationId) -> Vec<Effect<A>> {
        if !self.owns(operation, PhaseKind::CloseFailed) {
            return vec![];
        }
        let Some(next) = self.operation() else {
            return vec![Effect::Rejected(Rejection::IdentityExhausted)];
        };
        self.phase = Phase::Closing {
            id: next,
            discard_failed: true,
        };
        Self::changed(vec![Effect::FlushClose {
            operation: next,
            discard_failed: true,
            retry: false,
        }])
    }

    /// Cancel delivery/continuation, not an already-started disk write. A later
    /// matching write may still advance the baseline, but cannot resume action.
    /// Filesystem moves cannot be abandoned mid-flight: their completion must
    /// remap the retained document before editing/saving becomes available.
    pub fn cancel(&mut self, operation: OperationId, observed: Observed) -> Vec<Effect<A>> {
        if !self.matches(observed)
            || !self.foreground().is_some_and(|(id, kind)| {
                id == operation
                    && !matches!(
                        kind,
                        PhaseKind::Closed | PhaseKind::Moving | PhaseKind::EntryOperation
                    )
            })
        {
            return vec![];
        }
        let action = match std::mem::replace(&mut self.phase, Phase::Idle) {
            Phase::Prompt { next, .. }
            | Phase::Waiting {
                next: Waiting::Request(next, _),
                ..
            } => Some(next),
            Phase::Waiting {
                next: Waiting::Save(next),
                ..
            }
            | Phase::Writing { next, .. } => next,
            _ => None,
        };
        let mut effects = vec![Effect::Cancelled { operation, action }];
        self.resume(observed, &mut effects);
        Self::changed(effects)
    }
}

#[cfg(test)]
mod tests;
