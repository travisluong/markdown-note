# Linux launcher

No Linux binary release is available yet. If you build Markdown Note from source,
install the resulting `markdown-note` executable somewhere on your PATH, such
as `~/.local/bin`. From the repository root, install the checked-in launcher
and icon for your user:

```bash
install -Dm644 packaging/linux/io.github.travisluong.MarkdownNote.desktop ~/.local/share/applications/io.github.travisluong.MarkdownNote.desktop
install -Dm644 assets/brand/markdown-note.png ~/.local/share/icons/hicolor/512x512/apps/io.github.travisluong.MarkdownNote.png
```
