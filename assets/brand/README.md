# Application icons

`markdown-note.svg` is an exact copy of the marketing website's
`public/brand/markdown-note-mark.svg`. ICO, PNG and ICNS files are generated from that
mark, with transparency preserved. Windows embeds resource 1 in `markdown-note.exe`
for Explorer, shortcuts and the taskbar; the installer uses the same ICO. macOS
bundles the ICNS; Linux releases include the PNG and a desktop entry.

To regenerate after changing the marketing logo, install the marketing
repository's npm dependencies, then run from this repository:

```sh
node scripts/build-icons.mjs ../markdown-note-astro-poc
```

Commit the generated assets. Ordinary Cargo/release builds require neither Node
nor the marketing repository. The marketing repository's `build-brand.mjs`
generates its favicon from the same source mark.
