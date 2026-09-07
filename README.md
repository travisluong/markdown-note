# GPUI Note

A small Windows Markdown app built with GPUI. One window, one document.

## Run

Use stable Rust with the Windows MSVC toolchain, Visual Studio C++ Build Tools,
and a Windows SDK installed:

```powershell
cargo run --locked
```

## Use

- **File → Open…** (`Ctrl+O`): open a UTF-8 `.md` or `.markdown` file.
- **File → Save** (`Ctrl+S`): save back to the opened file.
- **View → Source mode**: edit plain Markdown, with standard copy/paste and undo/redo.
- **View → Reading view**: render the current text, including unsaved edits.
- `Alt+F` / `Alt+V`: open menus; use arrow keys, Enter, and Escape inside them.

An asterisk in the title means there are unsaved changes. Opening another file
or closing prompts to Save, Discard, or Cancel. Failed opens/saves preserve the
current buffer. Saves write a temporary file beside the original before replacing it.

UTF-8 BOM and LF/CRLF/CR line endings are preserved. Mixed line endings are
normalized to the first observed convention on save. Files without line endings
use LF. The file picker shows all files; unsupported extensions are rejected.

There is no New, Save As, autosave, file watching, tabs, or database. Reading view
uses GPUI Component's built-in Markdown support. This initial version is intended
for ordinary notes, and loads the whole document into memory.

## Check

```powershell
cargo check --locked
cargo test --locked
cargo fmt --check
```

The tests cover Unicode, empty files, BOM/newline round trips, dirty state,
invalid/missing files, and Windows replacement failures preserving the original.

For a UI smoke test, open a disposable Markdown file, edit it, switch both views,
save, and reopen. Check menu selection marks, keyboard navigation, scrolling,
clipboard, undo/redo, and each Save/Discard/Cancel branch on open and close.
Also check invalid UTF-8 and a locked/read-only file.
