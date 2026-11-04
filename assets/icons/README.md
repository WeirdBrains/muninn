# Icons

`muninn.png` is the **only** icon source. Every installer icon is generated from it at
build time by `scripts/icons.sh`; nothing generated is committed.

| Icon | Made by | Used for |
|---|---|---|
| `muninn.ico` (16–256 px) | `scripts/icons.sh ico` (Pillow or ImageMagick), called by `scripts/windows-setup.sh` | `setup.exe`, the uninstaller and Add/Remove Programs, the Start Menu entry |
| `muninn.icns` (16–1024 px) | `scripts/icons.sh icns` (`sips`, `iconutil`), called by `scripts/macos-app.sh` | `Muninn.app` and the `.dmg` volume icon |

The current image is a placeholder.

## Replacing the logo

1. Save the new logo over `assets/icons/muninn.png`: a square PNG, at least 1024×1024,
   with a transparent background if it isn't full-bleed. Keep the design readable at
   16 px.
2. Run `scripts/icons.sh check`, then `cargo test --test packaging`.
3. Rebuild the installers: `scripts/windows-setup.sh` (Linux, with the Windows host) and
   `scripts/macos-app.sh` (macOS). Commit only `muninn.png`.

Released installers keep the icon they were built with, so a new logo ships with the
next release.
