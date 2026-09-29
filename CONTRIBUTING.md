# Contributing to Markdown Note

Bug reports, small fixes, and documentation improvements are welcome. For a bug,
include your operating system, the app version, steps to reproduce, and a small
example when possible. Use synthetic notes and remove personal information from
screenshots or logs before sharing them.

## Set up a development build

Follow the [Windows prerequisites](README.md#build-from-source-windows), then run
these commands in Git Bash from the repository root:

```bash
cargo run --locked
```

The repository pins its Rust toolchain. All desktop assets and patched components
are included; a separate website, Node.js, database, or service is not needed to
build or edit notes. Optional in-app feedback uses the endpoint documented in
[feedback.md](docs/feedback.md).

## Understand the code

Start with [ARCHITECTURE.md](ARCHITECTURE.md) for ownership, document lifecycles,
and extension points. [AGENTS.md](AGENTS.md) records repository workflow rules.
Update the architecture map when changing ownership or public contracts.

Useful references:

- [Document sessions](docs/document-session.md) and [safe storage](docs/document-storage.md)
- [Commands and their entry points](docs/command-surface-inventory.md)
- [Performance diagnostics](benchmarks/README.md)
- [Windows packaging](packaging/windows/README.md)

Make focused changes and keep unrelated work separate. Do not edit Cargo's
registry cache. Local dependency changes belong in the checked-in `vendor/`
copies; preserve their licenses and update the relevant patch notes.

## Validate a change

The standard checks are:

```bash
cargo check --locked
cargo test --locked --features editor-diagnostics
cargo fmt --check
cargo clippy --locked --all-targets --features editor-diagnostics -- -D warnings
```

Choose focused tests while working, then run the required gates for the final
change. Report commands actually run, failures, and platform limitations. The
[progress log](docs/architecture-progress.md) contains historical evidence; it
does not substitute for validation of the current source.

The GPUI regression suite uses a headless test platform. Automated agents must
not launch the desktop app, run native UI harnesses, or inject desktop input.
Humans can check window behavior, native pickers, clipboard, physical keyboard
input, and installer behavior separately using disposable notes.

If a running app locks its executable, leave it running and use a separate target
directory for checks. Some Windows PTY signal tests depend on host permissions;
record a sandbox restriction separately from a passing test result.

Release-tooling changes also have Node.js tests. Node.js is needed for these
checks, not for ordinary desktop builds:

```bash
node --test .github/scripts/release.test.mjs scripts/release-local.test.mjs
```

An optional real-installer-compiler check may be skipped when its prerequisites
are unavailable. Follow the packaging guide for its prerequisites and record what
was verified. Running tests is separate from publishing a release.

## Submit your work

Explain the problem, the resulting behavior, and the checks you ran. Include
screenshots only when they help review a visible change. Keep each improvement
in a separate commit and preserve third-party notices.
