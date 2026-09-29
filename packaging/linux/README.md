# Linux launcher

The release archive includes the Markdown Note icon and desktop entry. After extracting
the archive, install `markdown-note` somewhere on your PATH (for example `~/.local/bin`),
then install the launcher and icon for your user:

```sh
install -Dm644 packaging/linux/io.github.travisluong.MarkdownNote.desktop ~/.local/share/applications/io.github.travisluong.MarkdownNote.desktop
install -Dm644 assets/brand/markdown-note.png ~/.local/share/icons/hicolor/512x512/apps/io.github.travisluong.MarkdownNote.png
```
