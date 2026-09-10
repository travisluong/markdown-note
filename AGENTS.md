# Agent instructions

- `vendor/gpui-component` is a checked-in copy of upstream 0.5.1 with local editor changes, selected by `[patch.crates-io]` in `Cargo.toml`. Keep its license and `EDITOR-PATCH.md`; reconcile local changes and rerun tests when upgrading. Never edit Cargo's registry cache.
- At the end of each prompt, stage and commit completed work with a concise message. Include only task-related changes; leave unrelated or unfinished work untouched. Do not push unless requested. If nothing changed, no commit is needed.
