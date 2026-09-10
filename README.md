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
- **View → Live preview**: edit headings, emphasis, and flat lists with reading typography.
- **View → Reading view**: render the current text, including unsaved edits.
- `Alt+F` / `Alt+V`: open menus; use arrow keys, Enter, and Escape inside them.

An asterisk in the title means there are unsaved changes. Opening another file
or closing prompts to Save, Discard, or Cancel. Failed opens/saves preserve the
current buffer. Saves write a temporary file beside the original before replacing it.

UTF-8 BOM and LF/CRLF/CR line endings are preserved. Mixed line endings are
normalized to the first observed convention on save. Files without line endings
use LF. The file picker shows all files; unsupported extensions are rejected.

There is no New, Save As, autosave, file watching, tabs, or database. Reading view
uses GPUI Component's built-in Markdown support. Source and Live preview share a rope buffer, selection, and undo history.

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

Live preview recognizes one to six leading `#` characters immediately, even
without a space. The hashes stay visible. `- ` becomes a bullet and `1. ` starts
an ordered item. Only unindented lists are supported. Matched `**bold**`,
`__bold__`, `*italic*`, and `_italic_` delimiters disappear off the cursor's source
line and reappear on that line. Unsupported syntax stays literal.

Enter splits a list item and continues its marker, incrementing an ordered
marker without renumbering other items. Enter on an empty list item clears that
line. Each list Enter is one undo step. Selections and pasted newlines use normal
replacement behavior. Saving and copying always use the original Markdown.

The pinned component is patched locally in `vendor/gpui-component`; see
[the patch notes](vendor/gpui-component/EDITOR-PATCH.md).
See [performance validation](benchmarks/README.md) for the release benchmark,
fixtures, hardware, limitations, and reproduction commands.
