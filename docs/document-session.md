# Document session contract

[`Session<D, I, A>`](../src/app/session.rs) is the sole owner of active content,
document identity, foreground phase, active document write, queued continuation,
autosave ticket and automatic retry policy. Its fields are private. The model
imports no GPUI, editor or application-root types and copies no source buffers.
The application supplies `Document`, image paths and `Next` as its generic values.

The live source remains in `InputState`. `Document` retains file metadata and the
saved text/rope baseline. Feature adapters read `Notes::document()`,
`image_path()` and `document_id()`; they cannot replace content through those
accessors. `Content` is exactly one of Empty, Document or Image.

## Owners and effects

| Responsibility | Owner / boundary |
| --- | --- |
| Pure decisions, private phase and identities | [session.rs](../src/app/session.rs) |
| Shared effects, save prompts, approved dispatch, content completion | [session_ui.rs](../src/app/session_ui.rs) |
| Timer handles, captured write work and error presentation | [autosave.rs](../src/autosave.rs) |
| Notebook resolution/load, preference barriers, close and move work | [notebook_ui.rs](../src/notebook_ui.rs) |
| Encoding, disk evidence and guarded replacement | [document storage contract](document-storage.md) |

Call a named transition, then apply its returned effects in order. The shared
adapter handles notifications, timers, document writes, prompts, approved
continuations and cancellation cleanup. Stage adapters consume effects requiring
their captured I/O data, such as Load, FlushActivation, FlushClose and Move.
They pass the remaining shared effects to `apply_session_effects`.

An adapter may retain an executor handle, error message, canonical target or
settings barrier paired with its operation token. Those are captured work and
presentation, not another busy flag or continuation owner. There is no general
“begin/finish operation” bypass. New asynchronous behavior needs a named model
event and a tested phase transition.

`StateChanged` refreshes dependent UI. This is required when close starts waiting
for settings: rendering must disable ordinary editor input before the window can
eventually disappear. Refresh notifications do not change session policy.

## Identity and foreground ownership

DocumentId, OperationId and WriteId are separate monotonic, non-wrapping types.
Installation creates a new content identity; moving preserves it. A WriteTicket
contains both its write identity and document identity. Every asynchronous
completion supplies its captured token to the matching model event.

There is one foreground phase. Background autosave may coexist with an auxiliary
picker, but its completion cannot release that picker. Navigation or manual Save
during a background write enters WaitingForWrite with one owned continuation.
After write completion, navigation checks actual dirtiness again; manual Save
still writes even when clean so it runs the disk-conflict guard.

| Flow | Accepted transitions |
| --- | --- |
| Navigation | request → optional write wait / Save-Discard-Cancel prompt → approved dispatch → picker or load → install / retained-content exit |
| Initial restore | Restoring → approved notebook dispatch or idle/error notice |
| Notebook activation | resolve canonical target → flush that target's settings → load → install; flush failures retain ownership for Retry or Cancel |
| Close | approved dispatch → flush all preference destinations → close; failure retains ownership for Retry, Cancel or explicit preference discard |
| Move | idle with no document write → Moving → matching path/metadata completion or retained-content failure |

Retrying close or activation receives a new operation token. Captured barrier
data and explicit same-notebook preference intents transfer to that token only
after the model accepts Retry. Older completions cannot install, release ownership
or consume the current continuation. An in-flight move cannot be cancelled:
the actual filesystem result must remap the retained content before editing or
saving resumes.

Close always waits for pending preference jobs, including submissions made while
its error prompt was open. “Close without saving preferences” permits ignoring
failed jobs only; it does not permit abandoning pending jobs. Cancellation keeps
the current editor and resumes eligible autosave.

## Dirty facts and captured saves

On an editor change, `edit_notice(document_id)` arms debounce without comparing
the whole source. A separate revision-tagged background comparison updates the
dirty display. At an actual save/navigation decision or debounce expiry,
`Observed` supplies the current document identity and an actual comparison with
the saved rope. Undo can therefore make the document clean despite a newer edit
revision. A revision number is never evidence that a write is necessary.

`Write` captures path, format, DiskVersion and one immutable rope snapshot.
Background work converts and saves that snapshot. `write_succeeded` accepts a
matching baseline exactly once; its callback updates saved text, saved rope and
receipt version and reports whether current text differs from the just-written
snapshot. Newer edits remain dirty. Baseline acceptance remains valid when the
original foreground continuation was cancelled, without releasing a newer owner.
A stale WriteTicket cannot release another write or consume its queued action.

On failure, the actual dirty fact compares against the unchanged old baseline.
An ordinary I/O failure waits for a later edit or explicit Save; it does not run
a retry loop. A disk conflict pauses automatic writes across later edits and
failed manual retries until a successful write or new installation. A move whose
new file version cannot be validated keeps its new path and buffer and applies
the same conflict pause.

Do not rearm autosave from an error-presentation handler. The model already emits
the retained-content ArmAutosave effect. Re-entering scheduling there can replace
its ticket before the outer effects finish, then install an obsolete timer over
the current one. `arm_session_autosave` also rejects a ticket that no longer
matches the model before replacing the GPUI handle. The delayed notebook-load
failure and stale-arm regressions cover actual eventual disk writes.

## Installation and dependent state

Only an accepted `loaded` event replaces content. The Installed effect creates a
fresh InputState and subscription, clears Reading find, resets dirty-display
work, configures image paths and view mode, and applies the installed document's
autosave settings. Switching Source/Live/Reading keeps the existing editor and
history; installation creates new history.

Every installation notifies `search_document_installed`, including clean document
navigation, image installation and empty content. The effect identifies the old
discarded document explicitly, so search can invalidate a captured unsaved overlay
without discarding unrelated clean search results. The adapter captures the old
path before model installation drops its metadata. Future dependent views should
respond to this accepted installation signal instead of observing incidental
field assignments.

Moving updates the current settings' paths, image-document metadata and search
state after the accepted completion. It never restores an old settings clone.
Snapshot-sensitive callbacks must retain their existing source/path guards:
DocumentId alone does not detect a move because a move preserves that identity.

## Extension and verification rules

- Add a pure transition test for the new event's accepted phase, stale completion,
  retained-content behavior and relevant failure/cancellation path.
- Add a TestPlatform case when correctness depends on actual InputState changes,
  prompt completion, timers, preference writes or installation notifications.
- Keep source comparisons at decision points. Do not add per-keystroke copies or
  use dirty-display state as a save decision.
- Keep errors as presentation data; route policy through model events and apply
  effects in order. Do not revive SaveState flags or mutable content accessors.
- Preserve disk-policy and metadata tests when changing captured write work.
  Native picker or desktop validation remains user-run under AGENTS.md.

The initial integration passed 29 pure transition tests and the complete Windows
diagnostics-enabled application target: **106 tests**, with no failures or ignored
tests. That target includes real TestPlatform close/input, queued save/navigation,
tree-retarget, conflict, move, preference and search cases. This records application
integration scope; it does not claim a new full-platform suite or native UI result.


The completed integration subsequently passed the full Windows and Linux suites,
301 tests on each platform, plus default builds and strict all-target lint.
See [final verification](architecture-progress.md#final-integrated-gates) for
counts, logs and platform limits.
