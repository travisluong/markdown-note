//! Terminal resize regression coverage uses only GPUI's in-memory TestPlatform.
use alacritty_terminal::{
    grid::Dimensions,
    term::{MIN_COLUMNS, MIN_SCREEN_LINES},
};
use gpui::{AppContext, Edges, px, size};
use gpui_terminal::{TerminalConfig, TerminalView, event::GpuiEventProxy, terminal::TerminalState};

#[test]
fn terminal_creation_uses_alacritty_minimum_dimensions() {
    for (cols, rows) in [(0, 0), (1, 1), (80, 24)] {
        let (tx, _events) = std::sync::mpsc::channel();
        let mut terminal = TerminalState::new(cols, rows, GpuiEventProxy::new(tx));
        let expected = (cols.max(MIN_COLUMNS), rows.max(MIN_SCREEN_LINES));
        assert_eq!((terminal.cols(), terminal.rows()), expected);
        terminal.with_term(|term| assert_eq!((term.columns(), term.screen_lines()), expected));
        terminal.process_bytes("茶".as_bytes());
    }
}

#[test]
fn terminal_single_column_resize_then_wide_output() {
    let (tx, _events) = std::sync::mpsc::channel();
    let mut terminal = TerminalState::new(80, 24, GpuiEventProxy::new(tx));
    terminal.process_bytes(b"prompt> ");
    terminal.resize(1, 24);
    terminal.process_bytes("茶".as_bytes());
    assert_eq!((terminal.cols(), terminal.rows()), (MIN_COLUMNS, 24));
    terminal.with_term(|term| assert_eq!((term.columns(), term.screen_lines()), (MIN_COLUMNS, 24)));
    terminal.resize(0, 0);
    assert_eq!(
        (terminal.cols(), terminal.rows()),
        (MIN_COLUMNS, MIN_SCREEN_LINES)
    );
    terminal.with_term(|term| {
        assert_eq!(
            (term.columns(), term.screen_lines()),
            (MIN_COLUMNS, MIN_SCREEN_LINES)
        )
    });
    terminal.process_bytes("界".as_bytes());
}

#[gpui::test]
fn terminal_explicit_resize_callback_matches_safe_grid_dimensions(cx: &mut gpui::TestAppContext) {
    let (_tx, rx) = flume::bounded(1);
    let resized = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorded = resized.clone();
    let terminal = cx.new(|cx| {
        TerminalView::from_receiver(std::io::sink(), rx, TerminalConfig::default(), cx)
            .with_resize_callback(move |cols, rows| recorded.lock().unwrap().push((cols, rows)))
    });
    terminal.update(cx, |terminal, _| terminal.resize(1, 3));
    cx.update(|cx| assert_eq!(terminal.read(cx).dimensions(), (MIN_COLUMNS, 3)));
    assert_eq!(*resized.lock().unwrap(), [(MIN_COLUMNS, 3)]);
    for dimensions in [(0, 0), (0, 3), (1, 0)] {
        terminal.update(cx, |terminal, _| {
            terminal.resize(dimensions.0, dimensions.1)
        });
    }
    cx.update(|cx| assert_eq!(terminal.read(cx).dimensions(), (MIN_COLUMNS, 3)));
    assert_eq!(*resized.lock().unwrap(), [(MIN_COLUMNS, 3)]);
}

#[gpui::test]
fn terminal_tiny_canvas_keeps_grid_geometry_and_pty_dimensions_safe(cx: &mut gpui::TestAppContext) {
    cx.update(gpui_component::init);
    let (tx, rx) = flume::bounded(64);
    let resized = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut terminal = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let recorded = resized.clone();
        let view = cx.new(|cx| {
            TerminalView::from_receiver(
                std::io::sink(),
                rx,
                TerminalConfig {
                    font_size: px(32.),
                    padding: Edges::all(px(8.)),
                    ..Default::default()
                },
                cx,
            )
            .with_resize_callback(move |cols, rows| recorded.lock().unwrap().push((cols, rows)))
        });
        terminal = Some(view.clone());
        gpui_component::Root::new(view, window, cx)
    });
    let terminal = terminal.unwrap();
    cx.simulate_resize(size(px(600.), px(300.)));
    cx.refresh().unwrap();
    tx.send(b"prompt> ".to_vec()).unwrap();
    cx.run_until_parked();
    // One pixel remains after padding: fewer than the two cells needed by fullwidth Unicode.
    cx.simulate_resize(size(px(17.), px(17.)));
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        let view = terminal.read(cx);
        let geometry = view.grid_geometry().unwrap();
        assert_eq!(view.dimensions(), (MIN_COLUMNS, MIN_SCREEN_LINES));
        assert_eq!((geometry.columns, geometry.rows), view.dimensions());
        assert_eq!(resized.lock().unwrap().last(), Some(&view.dimensions()));
    });
    tx.send("茶界".as_bytes().to_vec()).unwrap();
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.simulate_resize(size(px(600.), px(300.)));
    cx.refresh().unwrap();
    cx.update(|_, cx| assert!(terminal.read(cx).dimensions().0 > MIN_COLUMNS));
    assert!(
        resized
            .lock()
            .unwrap()
            .iter()
            .all(|&(cols, rows)| cols >= MIN_COLUMNS && rows >= MIN_SCREEN_LINES)
    );
}
