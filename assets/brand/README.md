# Application icons

`markdown-note.svg` is the source mark. The ICO, PNG and ICNS files are generated
from this local SVG with transparency preserved. Windows embeds resource 1 in
`markdown-note.exe` for Explorer, shortcuts and the taskbar; the installer uses
the same ICO. The macOS packaging references the ICNS, and the Linux launcher
references the PNG.

To regenerate after editing the SVG, install the optional image dependency into
an ignored directory and run the generator from the repository root in Git Bash:

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

Commit the generated PNG, ICO and ICNS files with the edited SVG. Ordinary Cargo
builds use the checked-in files and need neither Node nor these dependencies.
No website or private repository is needed to regenerate the icons.
