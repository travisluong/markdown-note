# Application icons

`markdown-note-icon-source.png` is the approved Pocket notebook icon: a dark
brown cover, cream page edge and tile, and a straight horizontal closure strap.
The area outside the rounded tile is transparent. `markdown-note-wordmark.png`
contains the matching logo with the lowercase "markdown note" name;
`generation.json` records the image-generation prompts.

The ICO, PNG and ICNS files are generated from the local icon source with
transparency preserved. Windows embeds resource 1 in
`markdown-note.exe` for Explorer, shortcuts and the taskbar; the installer uses
the same ICO. The macOS packaging references the ICNS, and the Linux launcher
references the PNG.

To regenerate after updating the source PNG, install the optional image dependency
into an ignored directory and run the generator from the repository root in Git Bash:

```bash
npm install --prefix target/icon-tools sharp
node scripts/build-icons.mjs
```

The generator defaults to `target/icon-tools` relative to the repository,
regardless of the current directory. An explicit dependency directory may be
passed as its first argument; it must contain an installed `sharp` package:

```bash
node scripts/build-icons.mjs 'C:/path/to/icon-tools'
```

Commit the generated PNG, ICO and ICNS files with the source PNG. Ordinary Cargo
builds use the checked-in files and need neither Node nor these dependencies.
No website or private repository is needed to regenerate the icons.

## Windows taskbar updates

The native window icon comes from resource 1 in the running executable. Exporting
an ICO elsewhere does not change it. `build.rs` watches `markdown-note.ico`, so
rebuild the executable after regenerating the icons:

```bash
cargo build --locked --bin markdown-note
cargo build --locked --release --bin markdown-note
```

Close an older running copy and launch the newly built executable. An installed
copy needs an updated installer or executable as well. If a pinned shortcut
still displays the old icon after updating, unpin it and pin the updated app
again. Agents verify embedded resources without launching or controlling the
desktop; checking the final Windows shell presentation remains manual.
