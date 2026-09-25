# Mithril

A desktop Markdown notebook built with GPUI, with Windows as the primary
platform. Notes are ordinary UTF-8 files, with Source, Live preview and Reading
views. See the [full guide](docs/guide.md) for usage and feature details, or
download a [Windows release](https://github.com/travisluong/mithril-releases/releases).

## Build and run

Install Rust through rustup, Visual Studio C++ Build Tools, and a Windows SDK.
Use the Windows MSVC toolchain; the repository pins Rust 1.98.1 in
[`rust-toolchain.toml`](rust-toolchain.toml). Run these commands from the repository
root in Git Bash:

```bash
cargo run --locked                  # Build and run a development build
cargo build --locked --release      # Build an optimized executable
./target/release/mithril.exe         # Run the optimized Windows build
```

## Key commands

```bash
cargo check --locked
cargo test --locked --features editor-diagnostics
cargo fmt --check
cargo clippy --locked --all-targets --features editor-diagnostics -- -D warnings
```

For Windows installers and publishing, follow the
[release procedure](packaging/windows/README.md).

In the app, use **Notebooks → Manage notebooks** to create or open a notebook.

| Shortcut | Action |
| --- | --- |
| Ctrl+O | Find a note or folder; open the file picker without a notebook. |
| Ctrl+P | Open the command palette, including **Notebook: Create new note**. |
| Ctrl+S | Save immediately (autosave is enabled by default). |
| Ctrl+F | Find in the current document. |
| Ctrl+Shift+F | Search the active notebook. |
| Ctrl+K | Insert or edit a link. |

On macOS, use Command instead of Ctrl for these shortcuts.
**Help → Keyboard shortcuts** lists the full bindings.

## Documentation

| Document | Contents |
| --- | --- |
| [Full guide](docs/guide.md) | Installation, getting started, notebooks, editing, search, settings and saving; the original README content. |
| [Mermaid diagrams](docs/mermaid.md) | Diagram syntax, editing and compatibility. |
| [Terminal](docs/terminal.md) | Terminal tabs, shell settings, shortcuts and limits. |
| [Feedback](docs/feedback.md) | Feedback behavior, contracts and verification. |
| [Windows releases](packaging/windows/README.md) | Build prerequisites, packaging, publishing and installer checks. |
| [Architecture](ARCHITECTURE.md) | Code ownership, lifecycle invariants and headless verification. |
| [Contributor instructions](AGENTS.md) | Repository workflow and agent rules. |
| [Document sessions](docs/document-session.md) | Session lifecycle and state transitions. |
| [Document storage](docs/document-storage.md) | Save behavior, conflict checks and storage guarantees. |
| [Command inventory](docs/command-surface-inventory.md) | Commands and their UI entry points. |
| [Performance validation](benchmarks/README.md) | Benchmarks, fixtures, measurements and manual diagnostics. |
| [Architecture progress](docs/architecture-progress.md) | Implementation and verification evidence. |
| [Architecture audit](docs/architecture-audit.md) | Original review and migration plan. |
