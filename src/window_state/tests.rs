use super::*;
use gpui::{Entity, TestAppContext, VisualTestContext, prelude::*};
use gpui_component::Root;

fn geometry(x: f32, y: f32, width: f32, height: f32) -> Geometry {
    Geometry {
        x,
        y,
        width,
        height,
    }
}

#[test]
fn serialization_missing_corrupt_and_atomic_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("window-state.json");
    assert_eq!(load(&path).unwrap(), None);
    let mut state = Placement {
        version: 1,
        maximized: true,
        normal: geometry(-1100., -200., 900., 700.),
    };
    save(&path, state).unwrap();
    assert_eq!(load(&path).unwrap(), Some(state));
    state.maximized = false;
    save(&path, state).unwrap();
    assert_eq!(load(&path).unwrap(), Some(state));
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    for bad in [
        "garbage",
        "{}",
        r#"{"version":2,"maximized":false,"normal":{"x":0,"y":0,"width":900,"height":700}}"#,
    ] {
        fs::write(&path, bad).unwrap();
        assert!(load(&path).is_err());
    }
    assert!(save(dir.path(), state).is_err());
}

#[test]
fn invalid_geometry_is_rejected_without_replacing_saved_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let valid = Placement {
        version: 1,
        maximized: false,
        normal: geometry(0., 0., 900., 700.),
    };
    save(&path, valid).unwrap();
    for normal in [
        geometry(0., 0., 0., 700.),
        geometry(0., 0., 900., -1.),
        geometry(f32::NAN, 0., 900., 700.),
        geometry(0., f32::INFINITY, 900., 700.),
        geometry(0., 0., f32::MAX, 700.),
    ] {
        assert!(save(&path, Placement { normal, ..valid }).is_err());
        assert_eq!(load(&path).unwrap(), Some(valid));
    }
    fs::write(
        &path,
        r#"{"version":1,"maximized":false,"normal":{"x":0,"y":0,"width":-1,"height":700}}"#,
    )
    .unwrap();
    assert!(load(&path).is_err());
}

#[test]
fn negative_coordinates_monitor_removal_and_minimum_size() {
    let primary = geometry(0., 0., 1920., 1080.);
    let secondary = geometry(-1600., -200., 1600., 900.);
    let normal = geometry(-1500., -100., 900., 700.);
    assert_eq!(normal.display(&[primary, secondary]), Some(1));
    assert_eq!(normal.display(&[primary]), Some(0));
    assert_eq!(normal.fit(&[primary, secondary]), normal);
    assert_eq!(normal.fit(&[primary]), geometry(0., 0., 900., 700.));
    assert_eq!(
        geometry(1800., 1000., 3000., 2000.).fit(&[primary]),
        primary
    );
    assert_eq!(
        geometry(1900., 1000., 20., 10.).fit(&[primary]),
        geometry(1520., 780., 400., 300.)
    );
    assert_eq!(normal.fit(&[]), normal);
}

#[test]
fn restore_normal_maximized_minimized_and_fullscreen() {
    let bounds = geometry(-1400., 0., 900., 700.).bounds();
    for (input, minimized, maximized) in [
        (WindowBounds::Windowed(bounds), false, false),
        (WindowBounds::Maximized(bounds), false, true),
        (WindowBounds::Windowed(bounds), true, true),
        (WindowBounds::Fullscreen(bounds), true, false),
    ] {
        let state = Placement::capture(input, minimized);
        let restored = state.restore(&[]);
        assert_eq!(restored.get_bounds(), bounds);
        assert_eq!(matches!(restored, WindowBounds::Maximized(_)), maximized);
        assert!(!matches!(restored, WindowBounds::Fullscreen(_)));
    }
}

#[gpui::test]
fn startup_defaults_invalid_state_and_diagnostic_bypass(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    cx.update(|cx| {
        let expected = Bounds::centered(None, size(px(900.), px(700.)), cx);
        assert!(state_path(true).is_none());
        assert_eq!(startup(None, cx).get_bounds(), expected);
        assert_eq!(startup(Some(&path), cx).get_bounds(), expected);
        fs::write(&path, "broken").unwrap();
        assert_eq!(startup(Some(&path), cx).get_bounds(), expected);
        for maximized in [true, false] {
            save(
                &path,
                Placement {
                    version: 1,
                    maximized,
                    normal: geometry(50., 60., 800., 600.),
                },
            )
            .unwrap();
            assert_eq!(
                matches!(startup(Some(&path), cx), WindowBounds::Maximized(_)),
                maximized
            );
            assert_eq!(
                startup(Some(&path), cx).get_bounds().size,
                size(px(800.), px(600.))
            );
            assert_eq!(
                display_for(startup(Some(&path), cx), cx),
                cx.primary_display().map(|display| display.id())
            );
        }
    });
}

fn fixture(
    cx: &mut TestAppContext,
    path: Option<PathBuf>,
) -> (Entity<Notes>, &mut VisualTestContext) {
    cx.update(gpui_component::init);
    let mut entity = None;
    let (_, view) = cx.add_window_view(|window, cx| {
        let notes = cx.new(|cx| {
            let mut notes = Notes::new(window, cx);
            notes.window_state_path = path;
            notes
        });
        entity = Some(notes.clone());
        Root::new(notes, window, cx)
    });
    (entity.unwrap(), view)
}

#[gpui::test]
fn cancelled_close_does_not_write_accepted_close_waits_for_placement(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("window-state.json");
    let note = dir.path().join("note.md");
    fs::write(&note, "original").unwrap();
    let (notes, cx) = fixture(cx, Some(path.clone()));
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.install_document(Some(crate::Document::open(note).unwrap()), window, cx);
            this.editor.replace_selection("changed", window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| notes.update(cx, |this, cx| this.request_window_close(window, cx)));
    cx.run_until_parked();
    assert!(!path.exists());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(!path.exists());
    assert_eq!(cx.windows().len(), 1);
    let captured = cx.update(|window, cx| {
        let path = path.clone();
        cx.on_window_closed(move |_| {
            // Observe the actual removal boundary, not just eventual task completion.
            assert!(load(&path).unwrap().is_some());
        })
        .detach();
        let bounds = window.window_bounds().get_bounds();
        notes.update(cx, |this, cx| this.request_window_close(window, cx));
        bounds
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    assert!(cx.windows().is_empty());
    assert_eq!(load(&path).unwrap().unwrap().normal.bounds(), captured);
}

#[gpui::test]
fn close_without_notebook_write_failure_and_disabled_persistence(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let blocked = dir.path().join("blocked");
    fs::write(&blocked, "file blocks directory creation").unwrap();
    for path in [
        Some(dir.path().join("state.json")),
        Some(blocked.join("state.json")),
        None,
    ] {
        let (notes, cx) = fixture(cx, path.clone());
        cx.update(|window, cx| notes.update(cx, |this, cx| this.request_window_close(window, cx)));
        cx.run_until_parked();
        assert!(cx.windows().is_empty());
        if path
            .as_ref()
            .is_some_and(|p| p.parent() == Some(dir.path()))
        {
            assert!(load(path.as_deref().unwrap()).unwrap().is_some());
        }
    }
    assert_eq!(
        fs::read_to_string(blocked).unwrap(),
        "file blocks directory creation"
    );
}
