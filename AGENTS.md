# Agent instructions

- Start architecture or feature work with `ARCHITECTURE.md` for current ownership, lifecycle invariants, and verification commands. `docs/architecture-audit.md` records the original review and migration plan; `docs/architecture-progress.md` records implementation and verification evidence. Update the architecture map when ownership or contracts change.
- `vendor/gpui-component` is a checked-in copy of upstream 0.5.1 with local editor changes, selected by `[patch.crates-io]` in `Cargo.toml`. Keep its license and `EDITOR-PATCH.md`; reconcile local changes and rerun tests when upgrading. Never edit Cargo's registry cache.
- At the end of each prompt, stage and commit completed work with a concise message. Include only task-related changes; leave unrelated or unfinished work untouched. Do not push unless requested. If nothing changed, no commit is needed.
- Make each improvement as a separate Git commit.
- Use Git Bash syntax for shell commands provided to the user unless they explicitly request another shell. Ensure paths, environment variables, and quoting work in Git Bash on Windows.
- Do not take over or interact with the user's desktop for testing. Do not use computer-use tools, inject mouse or keyboard input, change window focus, or launch visible apps for test purposes. Use unit tests, integration tests, static checks, and noninteractive/headless diagnostics instead. Leave any remaining manual UI validation to the user.
