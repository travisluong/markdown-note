//! Opt-in, in-app release benchmark. No user files are opened or saved.
use super::*;
use gpui_component::input::{Position, editor_work};
use std::time::Instant;

struct Run {
    step: usize,
    samples: Vec<(usize, f64, u64, u64)>,
    output: PathBuf,
}

pub(super) fn start(notes: &Entity<Notes>, output: PathBuf, window: &mut Window, cx: &mut App) {
    let _ = fs::write(
        output.with_extension("progress.txt"),
        "checking input behavior",
    );
    TextView::diagnostic_task_mappings(cx);
    let diagnostic = cx.new(|cx| InputState::new(window, cx).multi_line(true));
    diagnostic.update(cx, |editor, cx| editor.diagnostic_checks(window, cx));
    let fixture = format!(
        "#{}\n- **café** _你好_\n##Wrapped heading\nplain **bold** text\n> **quoted café** _你好_\n> > - [X] nested task\n>>Unicode quote wrapping words 你好 café words repeated repeated repeated repeated repeated repeated repeated repeated repeated repeated repeated repeated repeated repeated repeated repeated repeated repeated",
        "Heading words ".repeat(16)
    );
    diagnostic.update(cx, |editor, cx| {
        editor.set_value(fixture.clone(), window, cx);
        editor.set_cursor_position(Position::new(0, 0), window, cx);
    });
    notes.update(cx, |notes, cx| {
        notes.saves.session.enabled = false;
        notes.editor_subscription = Notes::subscribe_editor(&diagnostic, window, cx);
        notes.editor = diagnostic;
        notes.document = Some(Document {
            path: "diagnostics.md".into(),
            saved_rope: Rope::from(fixture.as_str()),
            saved: fixture,
            bom: false,
            newline: Newline::Lf,
        });
        notes.set_mode(ViewMode::LivePreview, window, cx);
    });
    geometry_round(notes.clone(), output, 0, window);
}
fn geometry_round(notes: Entity<Notes>, output: PathBuf, round: usize, window: &mut Window) {
    window.on_next_frame(move |window, cx| {
        notes.update(cx, |notes, cx| {
            notes
                .editor
                .update(cx, |editor, cx| editor.diagnostic_geometry(window, cx));
        });
        if round < 2 {
            notes.update(cx, |notes, cx| {
                let mut appearance = notes.appearance();
                appearance.content_font_size = if round == 0 { 20 } else { 16 };
                appearance.dark_mode = Some(round == 0);
                notes.update_appearance(appearance, window, cx);
                notes.editor.update(cx, |editor, cx| {
                    editor.set_cursor_position(Position::new(4, 0), window, cx);
                    editor.diagnostic_scroll(-24., cx);
                });
            });
            geometry_round(notes, output, round + 1, window);
            return;
        }
        notes.update(cx, |notes, cx| install(notes, false, window, cx));
        advance(
            notes,
            Run {
                step: 0,
                samples: Vec::new(),
                output,
            },
            window,
            cx,
        );
    });
}
fn install(notes: &mut Notes, long: bool, window: &mut Window, cx: &mut Context<Notes>) {
    let line = "Paragraph **bold** and _italic_ café 你好. The quick brown fox jumps over the lazy dog. More text here.\n";
    let text = if long {
        "long **bold** _italic_ café 你好 ".repeat(32_768)
    } else {
        line.repeat(100_000)
    };
    let editor = cx.new(|cx| {
        InputState::new(window, cx)
            .multi_line(true)
            .default_value(text.clone())
    });
    notes.editor_subscription = Notes::subscribe_editor(&editor, window, cx);
    notes.editor = editor;
    notes.document = Some(Document {
        path: "benchmark.md".into(),
        saved_rope: Rope::from(text.as_str()),
        saved: text,
        bom: false,
        newline: Newline::Lf,
    });
    notes.set_mode(ViewMode::LivePreview, window, cx);
}

fn advance(notes: Entity<Notes>, mut run: Run, window: &mut Window, cx: &mut App) {
    // A setup frame precedes each group; sample the following complete render frame.
    if run.step == 480 {
        let mut output =
            String::from("fixture,operation,p95_ms,max_parsed_lines,max_shaped_bytes\n");
        for group in 0..8 {
            let mut samples = run
                .samples
                .iter()
                .filter(|(g, _, _, _)| *g == group)
                .collect::<Vec<_>>();
            samples.sort_by(|a, b| a.1.total_cmp(&b.1));
            let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)].1;
            let parsed = samples.iter().map(|x| x.2).max().unwrap();
            let shaped = samples.iter().map(|x| x.3).max().unwrap();
            output.push_str(&format!(
                "{},{},{p95:.3},{parsed},{shaped}\n",
                ["beginning", "middle", "end", "long-line"][group / 2],
                if group % 2 == 0 { "edit" } else { "scroll" }
            ));
        }
        output.push_str("diagnostics,passed,,,\n");
        fs::write(&run.output, output).expect("write benchmark result");
        cx.quit();
        return;
    }
    let group = run.step / 60;
    let _ = fs::write(
        run.output.with_extension("progress.txt"),
        format!("frame {} of 480", run.step),
    );
    let sample = run.step % 60;
    if sample == 0 {
        notes.update(cx, |notes, cx| {
            if group == 6 {
                install(notes, true, window, cx);
            }
            notes.editor.update(cx, |editor, cx| {
                editor.set_cursor_position(
                    Position::new(
                        [0, 0, 50_000, 50_000, 99_950, 99_950, 0, 0][group],
                        if group >= 6 { 400_000 } else { 20 },
                    ),
                    window,
                    cx,
                );
            });
        });
    }
    let before = editor_work();
    let started = Instant::now();
    notes.update(cx, |notes, cx| {
        notes.editor.update(cx, |editor, cx| {
            if group % 2 == 0 {
                editor.insert("x", window, cx);
            } else {
                editor.diagnostic_scroll(if sample % 20 < 10 { -80. } else { 80. }, cx);
            }
        })
    });
    window.on_next_frame(move |window, cx| {
        let after = editor_work();
        if sample > 5 {
            run.samples.push((
                group,
                started.elapsed().as_secs_f64() * 1000.,
                after.parsed_lines - before.parsed_lines,
                after.shaped_bytes - before.shaped_bytes,
            ));
        }
        run.step += 1;
        advance(notes, run, window, cx);
    });
}
