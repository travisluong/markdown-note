# Markdown Note parser patch

This is the published `merman-core` 0.7.0 crate from crates.io, based on upstream
commit `00a2f291025913d612b6ca1ce9b6df6daf5bef94` in
https://github.com/Latias94/merman (crate path `crates/merman-core`).
The root Cargo patch selects it for both Merman and Merman Render.

Upstream source, tests, manifest, build script and README are retained. Cargo's
registry markers and packaged lockfile are omitted. The dual-license texts are
copied from the same upstream commit. Never edit Cargo's registry cache.

## Sequence participant whitespace (2026-09-22)

`src/diagrams/sequence/lexer.rs` preserves internal spaces and tabs in participant
identifiers, trims boundary whitespace, and stops at sequence delimiters and
comments. Declaration context alone recognizes the `as` alias separator. Numeric
prefixes in multiword names are kept together, while standalone numeric IDs
retain the existing conversion. Central-connection markers retain trimmed IDs.

No preprocessing or rewriting of document text is involved. Parsing errors,
strict rendering, source limits and resource restrictions remain in place.

The root `tests/mermaid_parser.rs` runs the upstream sequence unit tests alongside
local semantic regression cases. `tests/mermaid.rs` covers native rendering in
both themes plus Live and Reading source, copy and history behavior. All new
fixtures use synthetic names and messages; no user screenshots or original
diagram text are included.

Verify from the application root:

```text
cargo test --locked --offline --test mermaid_parser
cargo test --locked --offline --features editor-diagnostics --test mermaid
```

On upgrade, reconcile this lexer patch and rerun these suites and the application
checks before replacing or removing the Cargo patch.
