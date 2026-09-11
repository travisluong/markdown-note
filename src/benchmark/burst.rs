//! Opt-in real-cadence diagnostics. Invoked only by a user-run benchmark window.
use super::*;
use std::time::Duration;
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

#[derive(Default)]
struct Samples {
    pending: VecDeque<(u64, Instant)>,
    edit_ms: Vec<f64>,
    paint_ms: Vec<f64>,
    jobs: usize,
    max_gap_ms: f64,
    last_paint: Option<Instant>,
    done: bool,
}
fn observe(notes: Entity<Notes>, samples: Rc<RefCell<Samples>>, window: &mut Window) {
    window.on_next_frame(move |window, cx| {
        let (_, revision, painted, _) = notes.read(cx).editor.read(cx).diagnostic_preview_status();
        let mut s = samples.borrow_mut();
        if s.done {
            return;
        }
        if let Some(painted) = painted {
            if s.last_paint != Some(painted) {
                if let Some(last) = s.last_paint {
                    s.max_gap_ms = s
                        .max_gap_ms
                        .max(painted.duration_since(last).as_secs_f64() * 1000.);
                }
                s.last_paint = Some(painted);
            }
            while s
                .pending
                .front()
                .is_some_and(|(r, start)| *r <= revision && *start <= painted)
            {
                let (_, start) = s.pending.pop_front().unwrap();
                s.paint_ms
                    .push(painted.duration_since(start).as_secs_f64() * 1000.);
            }
        }
        drop(s);
        observe(notes, samples, window);
        window.refresh();
    });
}
fn p95(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    values
        .get(values.len() * 95 / 100)
        .copied()
        .unwrap_or(f64::NAN)
}
pub(super) fn start(notes: &Entity<Notes>, output: PathBuf, window: &mut Window, cx: &mut App) {
    let notes = notes.clone();
    let samples = Rc::new(RefCell::new(Samples::default()));
    observe(notes.clone(), samples.clone(), window);
    window.spawn(cx, async move |cx| {
        let mut csv = String::from("fixture,position,hz,edit_p95_ms,scheduled_to_paint_p95_ms,max_paint_gap_ms,max_jobs,unpainted,ui_copied_bytes,ui_parsed_bytes,table_rows_visited,cache_invalidations\n");
        for fixture in 0..3 {
            for position in 0..3 {
                for hz in [30, 60] {
                    let source = match fixture {
                        0 => "ordinary café 你好 text\n".repeat(100_000),
                        1 => "[café](note.md)\n\n".repeat(10_000),
                        _ => format!("before\n\n\n| name | value |\n| --- | --- |\n{}\n\nafter", "| café | 你好 |\n".repeat(10_000)),
                    };
                    cx.update(|window, cx| notes.update(cx, |notes, cx| {
                        notes.saves.session.enabled = false;
                        notes.editor.update(cx, |editor, cx| editor.set_value(source.clone(), window, cx));
                        notes.set_mode(ViewMode::LivePreview, window, cx);
                        let row = match (fixture, position) {
                            (_, 0) => 0,
                            (0, 1) => 50_000, (0, _) => 99_999,
                            (1, 1) => 10_000, (1, _) => 19_998,
                            (_, 1) => 2, _ => 10_007,
                        };
                        notes.editor.update(cx, |editor, cx| editor.set_cursor_position(Position::new(row, 0), window, cx));
                    })).unwrap();
                    // Warm-up waits for actual background analysis, not a synchronous substitute.
                    let deadline = Instant::now() + Duration::from_secs(60);
                    loop {
                        Timer::after(Duration::from_millis(20)).await;
                        let ready = notes.read_with(cx, |notes, cx| {
                            notes.editor.read(cx).diagnostic_preview_settled()
                        }).unwrap();
                        if ready { break; }
                        assert!(Instant::now() < deadline, "preview warm-up timed out");
                    }
                    *samples.borrow_mut() = Samples::default();
                    let before = editor_work();
                    let started = Instant::now();
                    for step in 0..hz * 4 {
                        let scheduled = started + Duration::from_secs_f64(step as f64 / hz as f64);
                        Timer::after(scheduled.saturating_duration_since(Instant::now())).await;
                        cx.update(|window, cx| notes.update(cx, |notes, cx| notes.editor.update(cx, |editor, cx| {
                            let before_revision = editor.diagnostic_preview_status().0;
                            let start = Instant::now();
                            editor.diagnostic_repeat_key(step < hz * 2, window, cx);
                            let (revision, _, _, jobs) = editor.diagnostic_preview_status();
                            let mut s = samples.borrow_mut();
                            s.edit_ms.push(start.elapsed().as_secs_f64() * 1000.);
                            s.jobs = s.jobs.max(jobs);
                            assert!(jobs <= 2, "unbounded analysis queue");
                            if revision != before_revision { s.pending.push_back((revision, scheduled)); }
                        }))).unwrap();
                    }
                    Timer::after(Duration::from_millis(500)).await;
                    let after = editor_work();
                    let mut s = samples.borrow_mut();
                    let edit = p95(&mut s.edit_ms);
                    let paint = p95(&mut s.paint_ms);
                    csv.push_str(&format!("{},{},{hz},{edit:.3},{paint:.3},{:.3},{},{},{},{},{},{}\n",
                        ["plain-100k", "links-10k", "table-10k"][fixture], ["beginning", "middle", "end"][position],
                        s.max_gap_ms, s.jobs, s.pending.len(), after.ui_copied_bytes - before.ui_copied_bytes,
                        after.ui_parsed_bytes - before.ui_parsed_bytes, after.table_rows_visited - before.table_rows_visited,
                        after.cache_invalidations - before.cache_invalidations));
                    fs::write(&output, &csv).unwrap();
                }
            }
        }
        samples.borrow_mut().done = true;
        cx.update(|_, cx| cx.quit()).unwrap();
    }).detach();
}
