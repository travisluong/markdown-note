# Linux launcher

The release archive includes the Mithril icon and desktop entry. After extracting
the archive, install `mithril` somewhere on your PATH (for example `~/.local/bin`),
then install the launcher and icon for your user:

```sh
install -Dm644 packaging/linux/org.mithril.Mithril.desktop ~/.local/share/applications/org.mithril.Mithril.desktop
install -Dm644 assets/brand/mithril.png ~/.local/share/icons/hicolor/512x512/apps/org.mithril.Mithril.png
```
