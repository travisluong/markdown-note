# Document session contract

[`Session<D, I, A>`](../src/app/session.rs) owns its tab's content and foreground
phase/continuation. Its private SavePolicy is shared by all views of one document
and owns the write, debounce ticket and retry pause. The pure model imports no
GPUI, editor or application-root types and copies no source buffers.

The application registry in `shared_documents.rs` owns the canonical document's
stable ID, shared baseline metadata, InputBuffer, buffer subscription, timer/error
and dirty comparison task. Tabs retain independent InputState views, modes,
Reading/find state and navigation histories. InputBuffer owns source/history;
view-local ropes are synchronized projections. Content remains exactly one of
Empty, Document or Image. Empty/image views have no shared document runtime.

## Owners and effects

A shared write result is accepted once by `write_succeeded`/`write_failed`.
`shared_write_finished` releases other matching tab continuations without accepting
the receipt again. Autosave cannot start while any view of that document owns
foreground work. Closing the originating view never invalidates retained-document
callbacks. Buffer events drive dirty/search/Reading updates so queued edits survive
immediate navigation away from their origin.

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

TabId identifies a view. Registry document IDs are stable, monotonic and independent
of tabs. Background callbacks capture the registry ID plus the document/write/path
or revision evidence, resolving surviving views at completion. Foreground callbacks
also capture TabId and OperationId. Session-local content/write identities may
collide across unrelated documents, so they never substitute for the registry ID.
Attaching a view binds it to the retained document's content identity and SavePolicy;
leaving it creates a fresh policy for its new content. Moves preserve identities.

Navigation opens in the current tab even when another tab shows its destination.
Attachments use current in-memory source, including unsaved edits. Last-view exits
use ordinary replacement approval; other exits retain the shared document without
prompting. Workspace preparation deduplicates documents in first-tab order.

There is one foreground phase per tab; application guards prevent conflicting foreground work. Background autosave may coexist with an auxiliary
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
matching baseline exactly once; its callback updates the shared saved rope and
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
fresh InputState view and link subscription, clears view-local Reading find,
configures image paths and mode, and attaches the document runtime. Opening an
already-retained document preserves its buffer, baseline and history. Only the
first load creates a new buffer/history. Switching Source/Live/Reading preserves
the current view's editor and shared history.

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

## Sidebar entry operations

`request_retaining` waits for in-flight writes while retaining the current draft;
its pending-write continuation retains that policy even when newer text exists.
Creating a note uses normal navigation approval. Trashing the current file or an
ancestor uses Save/Discard/Cancel; unrelated Trash and Duplicate do not prompt
for the active draft. Rename reuses the existing move lifecycle.

An approved filesystem action enters `EntryOperation`. It cannot be cancelled
mid-I/O, and only its operation token can finish it. Failures retain content and
history and resume eligible autosave. Successful deletion installs Empty only
when the deleted entry contained the active content; successful note creation
installs the new document. Installation forwards the exact discarded document
path to search. Preference changes apply to current settings after completion.

## File navigation history

`NavigationHistory` owns session-only canonical file visits independently of
editor undo. `Next::History(Visit)` carries a revision-tagged proposal through the
normal write barrier, Save/Discard/Cancel decision and loading operation. The
adapter captures that visit with OperationId at approved dispatch; the shared
accepted-install hook commits it only after matching content installation.
Cancelled, failed and stale work cannot move the cursor. Ordinary file/image
installs record visits; repeated current paths are ignored and a new visit after
Back drops the forward branch. Empty installation clears history. Sidebar note
creation calls the same hook. Successful file/folder moves remap stored paths
without creating a visit. History does not persist to notebook settings.
## Retained tabs and workspace preparation

TabSet owns one active tab and retained inactive tabs in strip order. User selection
moves ownership without recreating the Editor or Reading TextView. Background
completions resolve their captured registry document ID to surviving views; the
active slot is never temporarily swapped for an inactive callback. Foreground completions reject a
different active TabId. Any foreground phase or workspace coordinator prevents
user tab changes, while background autosaves continue during ordinary switches.

Workspace preparation uses Session::request or request_retaining with a dedicated
Prepared continuation. continued releases each approved session without replacing
its content. Autosave is disabled for the barrier, so neither discarded buffers nor
late write results restart timers. Close prepares each distinct document in first-tab order before
request_retaining(Close) enters the existing preference barrier. Cancellation or
failure keeps all tabs and unapplied discard decisions, while completed saves
retain their real disk baselines; eligible timers resume afterward.

Moves and Trash drain writes before filesystem work. Every retained session gets
its own operation token. Successful moves update every affected path and saved
disk version while preserving editor history. Failed revalidation pauses that
document's autosave. Trash installs Empty in affected sessions only after backend
success; failure/cancellation preserves buffers and histories.

Notebook activation uses activation_retained only for a matching LoadingNotebook
operation when it preserves an occupied tab. A new or reused empty tab uses the
normal accepted installation hook, attaching to shared in-memory content when
available. All ordinary note navigation stays in the current tab and commits only
that tab's history. Other views keep their histories, selections and modes.
