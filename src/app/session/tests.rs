use super::*;

#[derive(Debug, PartialEq, Eq)]
struct Document {
    path: &'static str,
    baseline: &'static str,
}

type Model = Session<Document, &'static str, &'static str>;

fn observed(model: &Model, dirty: bool) -> Observed {
    Observed {
        document: model.content().document_id(),
        dirty,
    }
}

fn relevant(effects: Vec<Effect<&'static str>>) -> Vec<Effect<&'static str>> {
    effects
        .into_iter()
        .filter(|effect| *effect != Effect::StateChanged)
        .collect()
}

fn operation(model: &Model, kind: PhaseKind) -> OperationId {
    let (id, actual) = model.foreground().expect("foreground operation");
    assert_eq!(actual, kind);
    id
}

fn load(model: &mut Model, content: Loaded<Document, &'static str>) -> DocumentId {
    let fact = observed(model, false);
    model.request("open", fact);
    let id = operation(model, PhaseKind::Dispatching);
    model.dispatch(id, Stage::Load);
    model.loaded(id, Ok(content), fact);
    model.content().identity().unwrap()
}

fn document() -> Model {
    let mut model = Model::default();
    load(
        &mut model,
        Loaded::Document(Document {
            path: "a.md",
            baseline: "saved",
        }),
    );
    model
}

fn background_write(model: &mut Model) -> WriteTicket {
    let fact = observed(model, true);
    model.set_autosave(true, fact);
    let timer = model.autosave().unwrap();
    assert!(!model.is_busy());
    model.autosave_due(timer, fact);
    assert!(!model.is_busy());
    model.write().unwrap()
}

#[test]
fn installation_has_one_content_kind_and_never_reuses_identity() {
    let mut model = document();
    let first = model.content().identity().unwrap();
    let image = load(&mut model, Loaded::Image("image.png"));
    assert!(image.0 > first.0);
    assert_eq!(
        model.content(),
        &Content::Image {
            id: image,
            value: "image.png"
        }
    );
    let fact = observed(&model, false);
    model.request("empty", fact);
    let op = operation(&model, PhaseKind::Dispatching);
    model.dispatch(op, Stage::Load);
    assert!(
        relevant(model.loaded(op, Ok(Loaded::Empty), fact)).contains(&Effect::Installed {
            previous: Some(image),
            current: None,
            discarded_document: None,
        })
    );
    assert_eq!(model.content(), &Content::Empty);
    let last = load(
        &mut model,
        Loaded::Document(Document {
            path: "b.md",
            baseline: "new",
        }),
    );
    assert!(last.0 > image.0);
}

#[test]
fn undo_to_saved_text_needs_no_write_when_debounce_checks_actual_dirty() {
    let mut model = document();
    let dirty = observed(&model, true);
    model.set_autosave(true, dirty);
    let first = model.autosave().unwrap();
    model.edit_notice(dirty.document.unwrap());
    let second = model.autosave().unwrap();
    assert_ne!(first, second);
    let clean = observed(&model, false); // Undo compares equal despite later revision.
    model.edit_notice(clean.document.unwrap());
    let undo_timer = model.autosave().unwrap();
    assert!(model.autosave_due(first, dirty).is_empty());
    assert!(model.autosave_due(second, dirty).is_empty());
    assert!(relevant(model.autosave_due(undo_timer, clean)).is_empty());
    assert!(model.write().is_none());
    let effects = relevant(model.request("next", clean));
    let op = operation(&model, PhaseKind::Dispatching);
    assert_eq!(
        effects,
        vec![Effect::Continue {
            operation: op,
            action: "next"
        }]
    );
}

#[test]
fn manual_save_writes_even_when_clean_and_accepts_baseline_once() {
    let mut model = document();
    let effects = relevant(model.save(None));
    let write = model.write().unwrap();
    assert_eq!(effects, vec![Effect::Write(write)]);
    let mut calls = 0;
    assert_eq!(
        relevant(model.write_succeeded(write, |document| {
            calls += 1;
            document.baseline = "saved";
            false
        })),
        vec![Effect::BaselineAccepted(write)]
    );
    assert_eq!(calls, 1);
    assert!(!model.is_busy());
    assert!(
        model
            .write_succeeded(write, |_| panic!("duplicate baseline acceptance"))
            .is_empty()
    );
    model.save(None);
    assert!(model.write().unwrap().id.0 > write.id.0);
}

#[test]
fn queued_navigation_rechecks_dirty_after_accepting_the_written_snapshot() {
    let mut model = document();
    let write = background_write(&mut model);
    let dirty = observed(&model, true);
    model.request("next", dirty);
    let waiting = operation(&model, PhaseKind::WaitingForWrite);
    assert_eq!(
        relevant(model.request("another", dirty)),
        vec![Effect::Rejected(Rejection::Busy)]
    );
    let effects = relevant(model.write_succeeded(write, |document| {
        document.baseline = "captured";
        true // Current text changed again while background save ran.
    }));
    let prompt = operation(&model, PhaseKind::Prompt);
    assert_ne!(prompt, waiting);
    assert_eq!(
        effects,
        vec![Effect::BaselineAccepted(write), Effect::Prompt(prompt)]
    );
    assert!(model.answer(waiting, Choice::Discard, dirty).is_empty());
    model.answer(prompt, Choice::Cancel, dirty);
    assert!(!model.is_busy());
    assert!(model.autosave().is_some());
}

#[test]
fn queued_navigation_with_clean_written_snapshot_continues_without_prompt() {
    let mut model = document();
    let write = background_write(&mut model);
    model.request("next", observed(&model, true));
    let effects = relevant(model.write_succeeded(write, |_| false));
    let op = operation(&model, PhaseKind::Dispatching);
    assert_eq!(
        effects,
        vec![
            Effect::BaselineAccepted(write),
            Effect::Continue {
                operation: op,
                action: "next"
            }
        ]
    );
    assert!(model.autosave().is_none());
}

#[test]
fn queued_manual_save_retries_the_guard_even_after_clean_background_success() {
    let mut model = document();
    let first = background_write(&mut model);
    model.save(None);
    let effects = relevant(model.write_succeeded(first, |_| false));
    let second = model.write().unwrap();
    let second_operation = operation(&model, PhaseKind::Writing);
    assert_ne!(first, second);
    assert_eq!(
        effects,
        vec![Effect::BaselineAccepted(first), Effect::Write(second)]
    );
    model.request("ignored", observed(&model, false));
    assert!(
        model
            .write_succeeded(first, |_| panic!("stale write"))
            .is_empty()
    );
    assert!(
        model
            .write_failed(first, SaveFailure::Io, observed(&model, false))
            .is_empty()
    );
    assert_eq!(model.write(), Some(second));
    assert_eq!(
        model.foreground(),
        Some((second_operation, PhaseKind::Writing))
    );
}

#[test]
fn stale_write_does_not_consume_a_new_writes_pending_action() {
    let mut model = document();
    let first = background_write(&mut model);
    model.write_succeeded(first, |_| false);
    let second = background_write(&mut model);
    model.request("retained next", observed(&model, true));
    let waiting = model.foreground();
    assert!(
        model
            .write_failed(first, SaveFailure::Conflict, observed(&model, true))
            .is_empty()
    );
    assert_eq!(model.foreground(), waiting);
    assert_eq!(model.write(), Some(second));
    let effects = relevant(model.write_succeeded(second, |_| false));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Continue {
            action: "retained next",
            ..
        }
    )));
}

#[test]
fn successful_background_save_does_not_release_an_unrelated_picker() {
    let mut model = document();
    let write = background_write(&mut model);
    model.pick();
    let picker = operation(&model, PhaseKind::Picking);
    assert_eq!(
        relevant(model.write_succeeded(write, |document| {
            document.baseline = "captured";
            true
        })),
        vec![Effect::BaselineAccepted(write)]
    );
    assert_eq!(model.foreground(), Some((picker, PhaseKind::Picking)));
    assert!(model.autosave().is_none());
    model.picked(picker, None, observed(&model, true));
    assert!(model.autosave().is_some());
}

#[test]
fn cancelling_queued_navigation_does_not_cancel_its_background_baseline() {
    let mut model = document();
    let write = background_write(&mut model);
    let fact = observed(&model, true);
    model.request("obsolete", fact);
    let waiting = operation(&model, PhaseKind::WaitingForWrite);
    model.cancel(waiting, fact);
    model.pick();
    let picker = operation(&model, PhaseKind::Picking);
    let effects = relevant(model.write_succeeded(write, |_| false));
    assert_eq!(effects, vec![Effect::BaselineAccepted(write)]);
    assert_eq!(model.foreground(), Some((picker, PhaseKind::Picking)));
    assert!(model.cancel(waiting, fact).is_empty());
}

#[test]
fn write_failure_has_no_retry_loop_but_next_edit_can_retry_io() {
    let mut model = document();
    let write = background_write(&mut model);
    let fact = observed(&model, true);
    assert_eq!(
        relevant(model.write_failed(write, SaveFailure::Io, fact)),
        vec![Effect::WriteFailed(write)]
    );
    assert!(model.autosave().is_none());
    model.pick();
    let picker = operation(&model, PhaseKind::Picking);
    model.picked(picker, None, fact);
    assert!(
        model.autosave().is_none(),
        "cancelling unrelated UI must not retry a failed write"
    );
    model.edit_notice(fact.document.unwrap());
    assert!(model.autosave().is_some());
}

#[test]
fn conflict_survives_edits_and_failed_manual_retry_until_successful_baseline() {
    let mut model = document();
    let write = background_write(&mut model);
    let fact = observed(&model, true);
    model.write_failed(write, SaveFailure::Conflict, fact);
    model.edit_notice(fact.document.unwrap());
    model.set_autosave(false, fact);
    model.set_autosave(true, fact);
    assert!(model.autosave().is_none());
    model.save(None);
    let retry = model.write().unwrap();
    model.write_failed(retry, SaveFailure::Io, fact);
    model.edit_notice(fact.document.unwrap());
    assert!(
        model.autosave().is_none(),
        "a failed explicit retry cannot clear a prior disk conflict"
    );
    model.save(None);
    let accepted = model.write().unwrap();
    model.write_succeeded(accepted, |_| true);
    assert!(
        model.autosave().is_some(),
        "successful explicit save accepts a new disk baseline"
    );
}

#[test]
fn failed_background_save_preserves_queued_request_or_one_explicit_retry() {
    let mut model = document();
    let first = background_write(&mut model);
    let fact = observed(&model, true);
    model.request("next", fact);
    let effects = relevant(model.write_failed(first, SaveFailure::Io, fact));
    let prompt = operation(&model, PhaseKind::Prompt);
    assert_eq!(
        effects,
        vec![Effect::WriteFailed(first), Effect::Prompt(prompt)]
    );
    model.answer(prompt, Choice::Save, fact);
    let retry = model.write().unwrap();
    model.write_failed(retry, SaveFailure::Io, fact);
    assert!(!model.is_busy());
    assert!(model.autosave().is_none());

    model.edit_notice(fact.document.unwrap());
    let timer = model.autosave().unwrap();
    model.autosave_due(timer, fact);
    let write = model.write().unwrap();
    model.save(None);
    let effects = relevant(model.write_failed(write, SaveFailure::Io, fact));
    let manual = model.write().unwrap();
    assert_eq!(
        effects,
        vec![Effect::WriteFailed(write), Effect::Write(manual)]
    );
    model.write_failed(manual, SaveFailure::Io, fact);
    assert!(!model.is_busy());
    assert!(model.write().is_none());
}

#[test]
fn discarded_navigation_cancel_error_and_tree_retarget_rearm_retained_edits() {
    for exit in ["picker", "error", "tree"] {
        let mut model = document();
        let fact = observed(&model, true);
        model.set_autosave(true, fact);
        model.request("open", fact);
        let prompt = operation(&model, PhaseKind::Prompt);
        model.answer(prompt, Choice::Discard, fact);
        let dispatch = operation(&model, PhaseKind::Dispatching);
        if exit == "picker" {
            model.dispatch(dispatch, Stage::Pick);
            model.picked(dispatch, None, fact);
        } else {
            model.dispatch(dispatch, Stage::Load);
            if exit == "tree" {
                model.retarget_load(dispatch, None);
                model.loaded(dispatch, Ok(Loaded::Empty), fact);
            } else {
                model.loaded(dispatch, Err(()), fact);
                assert!(model.autosave().is_none());
                assert_eq!(model.foreground(), Some((dispatch, PhaseKind::Error)));
                model.cancel(dispatch, fact);
            }
        }
        assert!(matches!(
            model.content(),
            Content::Document {
                value: Document { path: "a.md", .. },
                ..
            }
        ));
        assert!(!model.is_busy());
        assert!(
            model.autosave().is_some(),
            "{exit} must retain and resume edits"
        );
    }
}

#[test]
fn retargeted_load_uses_only_latest_target_and_rejects_duplicate_completion() {
    let mut model = document();
    let fact = observed(&model, false);
    model.request("A", fact);
    let first = operation(&model, PhaseKind::Dispatching);
    model.dispatch(first, Stage::Load);
    model.retarget_load(first, Some("B"));
    model.retarget_load(first, Some("C"));
    let effects = relevant(model.loaded(first, Err(()), fact));
    let latest = operation(&model, PhaseKind::Dispatching);
    assert_ne!(latest, first);
    assert_eq!(
        effects,
        vec![Effect::Continue {
            operation: latest,
            action: "C"
        }]
    );
    model.dispatch(latest, Stage::Load);
    assert!(model.loaded(first, Ok(Loaded::Empty), fact).is_empty());
    assert_eq!(model.foreground(), Some((latest, PhaseKind::Loading)));
    model.loaded(latest, Ok(Loaded::Image("c.png")), fact);
    assert!(matches!(
        model.content(),
        Content::Image { value: "c.png", .. }
    ));
}

#[test]
fn move_preserves_document_identity_baseline_and_resumes_dirty_autosave() {
    let mut model = document();
    let identity = model.content().document_id().unwrap();
    let fact = observed(&model, true);
    model.set_autosave(true, fact);
    model.move_content();
    let op = operation(&model, PhaseKind::Moving);
    assert!(model.autosave().is_none());
    assert!(
        model.cancel(op, fact).is_empty(),
        "an in-flight filesystem move cannot safely be abandoned"
    );
    assert_eq!(model.foreground(), Some((op, PhaseKind::Moving)));
    model.moved(op, |content| {
        let MovingContent::Document(document) = content else {
            panic!("document")
        };
        document.path = "folder/a.md";
        MoveOutcome {
            dirty: true,
            conflict: false,
        }
    });
    assert_eq!(
        model.content(),
        &Content::Document {
            id: identity,
            value: Document {
                path: "folder/a.md",
                baseline: "saved"
            }
        }
    );
    assert!(model.autosave().is_some());
    assert!(
        model
            .moved(op, |_| panic!("duplicate move mutation"))
            .is_empty()
    );
    let write = background_write(&mut model);
    assert_eq!(
        relevant(model.move_content()),
        vec![Effect::Rejected(Rejection::Busy)]
    );
    model.write_succeeded(write, |_| false);
    model.move_content();
    let failed = operation(&model, PhaseKind::Moving);
    model.move_failed(failed, fact);
    assert_eq!(model.content().document_id(), Some(identity));
    assert!(model.autosave().is_some());
}

#[test]
fn close_flush_owns_foreground_until_success_or_explicit_failure_cancel() {
    let mut model = document();
    let clean = observed(&model, false);
    model.request("close", clean);
    let op = operation(&model, PhaseKind::Dispatching);
    model.dispatch(op, Stage::Close);
    assert!(model.is_busy());
    assert_eq!(
        relevant(model.close_flushed(op, false)),
        vec![Effect::CloseFailed(op)]
    );
    assert!(model.is_busy());
    model.retry_close(op);
    let retry = operation(&model, PhaseKind::Closing);
    assert_ne!(retry, op);
    assert!(model.close_flushed(op, true).is_empty());
    assert_eq!(model.foreground(), Some((retry, PhaseKind::Closing)));
    model.close_flushed(retry, false);
    model.cancel(retry, clean);
    assert!(!model.is_busy());
    model.request("close again", clean);
    let last = operation(&model, PhaseKind::Dispatching);
    model.dispatch(last, Stage::Close);
    assert_eq!(
        relevant(model.close_flushed(last, true)),
        vec![Effect::Close]
    );
    assert_eq!(model.foreground(), Some((last, PhaseKind::Closed)));
    assert!(model.cancel(last, clean).is_empty());
}

#[test]
fn stale_observations_do_not_arm_or_replace_another_document() {
    let mut model = document();
    let old = observed(&model, true);
    load(
        &mut model,
        Loaded::Document(Document {
            path: "b.md",
            baseline: "B",
        }),
    );
    assert!(model.edit_notice(old.document.unwrap()).is_empty());
    assert!(model.set_autosave(true, old).is_empty());
    assert!(model.request("stale", old).is_empty());
    assert!(!model.is_busy());
    assert!(model.autosave().is_none());
}

#[test]
fn operation_write_timer_and_document_exhaustion_never_wrap() {
    let mut model = document();
    let fact = observed(&model, false);
    model.last_operation = u64::MAX;
    assert_eq!(
        relevant(model.request("next", fact)),
        vec![Effect::Rejected(Rejection::IdentityExhausted)]
    );
    assert!(!model.is_busy());
    assert_eq!(model.last_operation, u64::MAX);

    let mut model = document();
    model.saving.borrow_mut().last_write = u64::MAX;
    assert_eq!(
        relevant(model.save(None)),
        vec![Effect::Rejected(Rejection::IdentityExhausted)]
    );
    assert!(model.write().is_none());
    assert!(!model.is_busy());
    assert_eq!(model.saving.borrow().last_write, u64::MAX);

    let mut model = document();
    model.saving.borrow_mut().last_autosave = u64::MAX;
    let fact = observed(&model, true);
    assert_eq!(
        relevant(model.set_autosave(true, fact)),
        vec![Effect::Rejected(Rejection::IdentityExhausted)]
    );
    assert!(model.autosave().is_none());
    assert_eq!(model.saving.borrow().last_autosave, u64::MAX);

    let mut model = document();
    let prior = model.content().identity();
    model.last_document = u64::MAX;
    let fact = observed(&model, false);
    model.request("next", fact);
    let op = operation(&model, PhaseKind::Dispatching);
    model.dispatch(op, Stage::Load);
    assert_eq!(
        relevant(model.loaded(op, Ok(Loaded::Image("new.png")), fact)),
        vec![Effect::Rejected(Rejection::IdentityExhausted)]
    );
    assert_eq!(model.content().identity(), prior);
    assert_eq!(model.last_document, u64::MAX);
    assert!(!model.is_busy());
}

#[test]
fn failed_auxiliary_picker_and_completed_noop_resume_retained_content() {
    let mut model = document();
    let fact = observed(&model, true);
    model.set_autosave(true, fact);
    model.pick();
    let picker = operation(&model, PhaseKind::Picking);
    assert_eq!(
        relevant(model.pick_failed(picker)),
        vec![Effect::ShowError(picker)]
    );
    assert!(model.autosave().is_none());
    model.cancel(picker, fact);
    assert!(model.autosave().is_some());
    model.request("current note", fact);
    let prompt = operation(&model, PhaseKind::Prompt);
    let cancelled = relevant(model.answer(prompt, Choice::Cancel, fact));
    assert!(cancelled.contains(&Effect::Cancelled {
        operation: prompt,
        action: Some("current note")
    }));
    model.request("current note", fact);
    let prompt = operation(&model, PhaseKind::Prompt);
    model.answer(prompt, Choice::Discard, fact);
    let no_op = operation(&model, PhaseKind::Dispatching);
    model.continued(no_op, fact);
    assert!(!model.is_busy());
    assert!(model.autosave().is_some());
}

#[test]
fn moving_image_keeps_identity_and_new_document_clears_conflict_pause() {
    let mut model = document();
    let write = background_write(&mut model);
    let dirty = observed(&model, true);
    model.write_failed(write, SaveFailure::Conflict, dirty);
    model.request("image", dirty);
    let prompt = operation(&model, PhaseKind::Prompt);
    model.answer(prompt, Choice::Discard, dirty);
    let op = operation(&model, PhaseKind::Dispatching);
    model.dispatch(op, Stage::Load);
    model.loaded(op, Ok(Loaded::Image("old.png")), dirty);
    let identity = model.content().identity();
    model.move_content();
    let op = operation(&model, PhaseKind::Moving);
    model.moved(op, |content| {
        let MovingContent::Image(path) = content else {
            panic!("image")
        };
        *path = "folder/image.png";
        MoveOutcome {
            dirty: false,
            conflict: false,
        }
    });
    assert_eq!(
        model.content(),
        &Content::Image {
            id: identity.unwrap(),
            value: "folder/image.png"
        }
    );
    load(
        &mut model,
        Loaded::Document(Document {
            path: "new.md",
            baseline: "new",
        }),
    );
    model.edit_notice(model.content().document_id().unwrap());
    assert!(model.autosave().is_some());
}

#[test]
fn cancelling_failed_close_after_discard_rearms_unsaved_retained_text() {
    let mut model = document();
    let dirty = observed(&model, true);
    model.set_autosave(true, dirty);
    model.request("close", dirty);
    let prompt = operation(&model, PhaseKind::Prompt);
    model.answer(prompt, Choice::Discard, dirty);
    let op = operation(&model, PhaseKind::Dispatching);
    model.dispatch(op, Stage::Close);
    model.close_flushed(op, false);
    assert!(model.autosave().is_none());
    model.cancel(op, dirty);
    assert!(model.autosave().is_some());
    assert!(model.close_flushed(op, true).is_empty());
    assert!(matches!(model.content(), Content::Document { .. }));
}

#[test]
fn exhausted_prompt_continuation_still_releases_and_notifies_foreground() {
    let mut model = document();
    let dirty = observed(&model, true);
    model.request("next", dirty);
    let prompt = operation(&model, PhaseKind::Prompt);
    model.last_operation = u64::MAX;
    let effects = model.answer(prompt, Choice::Save, dirty);
    assert_eq!(
        effects,
        vec![
            Effect::Rejected(Rejection::IdentityExhausted),
            Effect::StateChanged
        ]
    );
    assert!(!model.is_busy());
    assert!(model.write().is_none());
    assert_eq!(model.last_operation, u64::MAX);
}

#[test]
fn initial_restore_has_real_ownership_and_preserves_failed_index() {
    let mut model = Model::default();
    let effects = relevant(model.restore());
    let first = operation(&model, PhaseKind::Restoring);
    assert_eq!(effects, vec![Effect::RestoreIndex(first)]);
    assert_eq!(
        relevant(model.restore()),
        vec![Effect::Rejected(Rejection::Busy)]
    );
    assert_eq!(
        relevant(model.restored(first, Err(()))),
        vec![Effect::RestoreFailed(first)]
    );
    assert_eq!(model.content(), &Content::Empty);
    assert!(!model.is_busy());

    model.restore();
    let second = operation(&model, PhaseKind::Restoring);
    assert!(
        model
            .restored(first, Ok(Some("obsolete notebook")))
            .is_empty()
    );
    let effects = relevant(model.restored(second, Ok(Some("active notebook"))));
    let dispatch = operation(&model, PhaseKind::Dispatching);
    assert_eq!(
        effects,
        vec![Effect::Continue {
            operation: dispatch,
            action: "active notebook"
        }]
    );
    model.dispatch(dispatch, Stage::ActivateNotebook);
    assert_eq!(
        model.foreground(),
        Some((dispatch, PhaseKind::ResolvingActivation))
    );
    model.cancel(dispatch, observed(&model, false));
    load(
        &mut model,
        Loaded::Document(Document {
            path: "a.md",
            baseline: "saved",
        }),
    );
    assert_eq!(
        relevant(model.restore()),
        vec![Effect::Rejected(Rejection::Busy)],
        "restore cannot replace installed content outside navigation"
    );
}

#[test]
fn standalone_error_coexists_with_background_save_and_dismisses_by_owner() {
    let mut model = document();
    let write = background_write(&mut model);
    let effects = relevant(model.show_error());
    let error = operation(&model, PhaseKind::Error);
    assert_eq!(effects, vec![Effect::ShowError(error)]);
    model.write_succeeded(write, |_| true);
    assert_eq!(model.foreground(), Some((error, PhaseKind::Error)));
    assert!(model.autosave().is_none());
    model.cancel(error, observed(&model, true));
    assert!(model.autosave().is_some());
    model.pick();
    let picker = operation(&model, PhaseKind::Picking);
    assert!(model.cancel(error, observed(&model, true)).is_empty());
    assert_eq!(model.foreground(), Some((picker, PhaseKind::Picking)));
}

fn activate(model: &mut Model, fact: Observed) -> OperationId {
    model.request("notebook B", fact);
    if let Some((prompt, PhaseKind::Prompt)) = model.foreground() {
        model.answer(prompt, Choice::Discard, fact);
    }
    let op = operation(model, PhaseKind::Dispatching);
    assert_eq!(
        relevant(model.dispatch(op, Stage::ActivateNotebook)),
        vec![Effect::ResolveActivation(op)]
    );
    op
}

#[test]
fn activation_resolution_and_load_failures_retain_and_resume_document() {
    for resolving in [true, false] {
        let mut model = document();
        let fact = observed(&model, true);
        let original = model.content().identity();
        model.set_autosave(true, fact);
        let op = activate(&mut model, fact);
        let effects = if resolving {
            model.activation_resolved(op, false, fact)
        } else {
            model.activation_resolved(op, true, fact);
            model.activation_flushed(op, true);
            model.loaded(op, Err(()), fact)
        };
        assert!(relevant(effects).contains(&Effect::ActivationFailed(op)));
        assert_eq!(model.content().identity(), original);
        assert!(!model.is_busy());
        assert!(model.autosave().is_some());
    }
}

#[test]
fn activation_waits_for_settings_and_retry_rejects_old_flush_completion() {
    let mut model = document();
    let fact = observed(&model, true);
    let original = model.content().identity();
    model.set_autosave(true, fact);
    let op = activate(&mut model, fact);
    assert_eq!(
        relevant(model.activation_resolved(op, true, fact)),
        vec![Effect::FlushActivation {
            operation: op,
            retry: false
        }]
    );
    assert_eq!(
        model.foreground(),
        Some((op, PhaseKind::FlushingActivation))
    );
    assert!(model.loaded(op, Ok(Loaded::Empty), fact).is_empty());
    assert_eq!(
        relevant(model.activation_flushed(op, false)),
        vec![Effect::ActivationFlushFailed(op)]
    );
    assert!(model.is_busy());
    assert!(model.autosave().is_none());
    let effects = relevant(model.retry_activation(op));
    let retry = operation(&model, PhaseKind::FlushingActivation);
    assert_ne!(retry, op);
    assert_eq!(
        effects,
        vec![Effect::FlushActivation {
            operation: retry,
            retry: true
        }]
    );
    assert!(model.activation_flushed(op, true).is_empty());
    assert_eq!(model.content().identity(), original);
    model.activation_flushed(retry, false);
    model.cancel(retry, fact);
    assert!(model.autosave().is_some());
    assert_eq!(model.content().identity(), original);
}

#[test]
fn successful_activation_installs_only_after_matching_flush_and_load() {
    let mut model = document();
    let fact = observed(&model, false);
    let original = model.content().identity();
    let op = activate(&mut model, fact);
    model.activation_resolved(op, true, fact);
    assert_eq!(
        relevant(model.activation_flushed(op, true)),
        vec![Effect::Load(op)]
    );
    assert_eq!(model.foreground(), Some((op, PhaseKind::LoadingNotebook)));
    assert!(
        model
            .retarget_load(op, Some("unrelated tree target"))
            .is_empty()
    );
    model.loaded(
        op,
        Ok(Loaded::Document(Document {
            path: "book-b/b.md",
            baseline: "B",
        })),
        fact,
    );
    assert_ne!(model.content().identity(), original);
    assert!(!model.is_busy());
    assert!(model.activation_flushed(op, false).is_empty());
}

#[test]
fn close_discard_is_explicit_and_still_waits_for_the_final_barrier() {
    let mut model = document();
    let clean = observed(&model, false);
    model.request("close", clean);
    let op = operation(&model, PhaseKind::Dispatching);
    let effects = relevant(model.dispatch(op, Stage::Close));
    assert_eq!(
        effects,
        vec![Effect::FlushClose {
            operation: op,
            discard_failed: false,
            retry: false,
        }]
    );
    assert!(model.discard_close_preferences(op).is_empty());
    model.close_flushed(op, false);
    let effects = relevant(model.discard_close_preferences(op));
    let discard = operation(&model, PhaseKind::Closing);
    assert_ne!(discard, op);
    assert_eq!(
        effects,
        vec![Effect::FlushClose {
            operation: discard,
            discard_failed: true,
            retry: false,
        }]
    );
    assert!(
        model.is_busy(),
        "explicit discard still waits for pending and late preference writes"
    );
    assert!(model.close_flushed(op, true).is_empty());
    assert_eq!(
        relevant(model.close_flushed(discard, false)),
        vec![Effect::Close]
    );
}

#[test]
fn moved_path_survives_version_failure_and_pauses_automatic_saves() {
    let mut model = document();
    let fact = observed(&model, true);
    let original = model.content().document_id().unwrap();
    model.set_autosave(true, fact);
    model.move_content();
    let op = operation(&model, PhaseKind::Moving);
    let effects = relevant(model.moved(op, |content| {
        let MovingContent::Document(document) = content else {
            panic!("document")
        };
        document.path = "moved/a.md";
        MoveOutcome {
            dirty: true,
            conflict: true,
        }
    }));
    assert_eq!(
        effects,
        vec![
            Effect::Moved(Some(original)),
            Effect::MoveBaselineConflict(original)
        ]
    );
    assert!(model.save_conflicted());
    assert_eq!(
        model.content(),
        &Content::Document {
            id: original,
            value: Document {
                path: "moved/a.md",
                baseline: "saved"
            }
        }
    );
    model.edit_notice(original);
    assert!(model.autosave().is_none());
    model.save(None);
    let write = model.write().unwrap();
    model.write_succeeded(write, |_| false);
    assert!(!model.save_conflicted());
}

#[test]
fn entry_deletion_keeps_content_until_success_and_rejects_stale_completions() {
    let mut model = document();
    let fact = observed(&model, true);
    model.request("trash", fact);
    let prompt = operation(&model, PhaseKind::Prompt);
    model.answer(prompt, Choice::Cancel, fact);
    assert!(model.content().document_id().is_some());
    model.request("trash", fact);
    let prompt = operation(&model, PhaseKind::Prompt);
    model.answer(prompt, Choice::Discard, fact);
    let op = operation(&model, PhaseKind::Dispatching);
    model.begin_entry_operation(op);
    assert!(model.is_busy());
    assert!(model.cancel(op, fact).is_empty());
    assert!(model.content().document_id().is_some());
    model.entry_deleted(op, false, true, fact);
    assert_eq!(model.content().document_id(), fact.document);
    model.request("trash", fact);
    let prompt = operation(&model, PhaseKind::Prompt);
    model.answer(prompt, Choice::Discard, fact);
    let next = operation(&model, PhaseKind::Dispatching);
    model.begin_entry_operation(next);
    assert!(model.entry_deleted(op, true, true, fact).is_empty());
    assert_eq!(model.content().document_id(), fact.document);
    let effects = model.entry_deleted(next, true, true, fact);
    assert!(matches!(model.content(), Content::Empty));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::Installed { current: None, .. }))
    );
}
#[test]
fn unrelated_entry_operation_waits_for_write_without_prompting_for_late_edits() {
    let mut model = document();
    let write = background_write(&mut model);
    let fact = observed(&model, true);
    model.request_retaining("duplicate", fact);
    operation(&model, PhaseKind::WaitingForWrite);
    let effects = model.write_succeeded(write, |_| true);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Prompt(_)))
    );
    let op = operation(&model, PhaseKind::Dispatching);
    model.begin_entry_operation(op);
    model.entry_operation_finished(op, None, fact);
    assert_eq!(model.content().document_id(), fact.document);
    assert!(model.autosave().is_some());
}

#[test]
fn notebook_activation_can_retain_content_only_for_the_matching_load() {
    let mut model = document();
    let fact = observed(&model, true);
    let identity = fact.document;
    model.set_autosave(true, fact);
    model.request_retaining("notebook", fact);
    let id = operation(&model, PhaseKind::Dispatching);
    assert!(model.activation_retained(id, fact).is_empty());
    model.dispatch(id, Stage::ActivateNotebook);
    model.activation_resolved(id, true, fact);
    model.activation_flushed(id, true);
    assert_eq!(model.foreground(), Some((id, PhaseKind::LoadingNotebook)));
    assert!(
        model
            .activation_retained(OperationId(id.0 + 1), fact)
            .is_empty()
    );
    let effects = model.activation_retained(id, fact);
    assert!(effects.iter().any(|e| matches!(e, Effect::ArmAutosave(_))));
    assert_eq!(model.content().document_id(), identity);
    assert!(!model.is_busy());
    assert!(model.activation_retained(id, fact).is_empty());
}
#[test]
fn shared_write_owner_serializes_views_and_releases_only_matching_continuations() {
    let mut first = document();
    let mut second = document();
    second.attach_saving(
        first.saving_policy(),
        first.content().document_id().unwrap(),
    );
    let write = background_write(&mut first);
    let fact = observed(&second, true);
    assert_eq!(second.write(), Some(write));
    second.request_retaining("leave second", fact);
    assert_eq!(operation(&second, PhaseKind::WaitingForWrite).0, 2);
    let effects = first.write_succeeded(write, |_| false);
    assert!(effects.contains(&Effect::BaselineAccepted(write)));
    let fact = observed(&second, false);
    let effects = second.shared_write_finished(write, true, fact);
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Continue {
            action: "leave second",
            ..
        }
    )));
    assert!(second.shared_write_finished(write, true, fact).is_empty());
    assert!(
        first
            .write_succeeded(write, |_| panic!("duplicate baseline"))
            .is_empty()
    );
}

#[test]
fn shared_conflict_pause_survives_edits_in_either_view() {
    let mut first = document();
    let mut second = document();
    second.attach_saving(
        first.saving_policy(),
        first.content().document_id().unwrap(),
    );
    let write = background_write(&mut first);
    let fact = observed(&first, true);
    first.write_failed(write, SaveFailure::Conflict, fact);
    assert!(second.save_conflicted());
    second.edit_notice(second.content().document_id().unwrap());
    assert!(first.autosave().is_none());
    assert!(second.autosave().is_none());
    second.save(None);
    assert!(first.write().is_some());
}
