# Icons

`muninn.svg` is the **only** icon source: the Muninn raven tile (by Matematik), used exactly
as delivered. Its fills are CSS variables with Tokyo Night fallbacks
(`--muninn-header`, `--muninn-accent`, `--muninn-bg`); renderers that ignore the variables
use the fallbacks, so the installer icons are Tokyo Night. Every icon is generated from it
at build time by `scripts/icons.sh`; nothing generated is committed.

| Icon | Made by | Used for |
|---|---|---|
| `muninn.ico` (16–256 px) | `scripts/icons.sh ico`, called by `scripts/windows-setup.sh` | `setup.exe`, the uninstaller and Add/Remove Programs, the Start Menu entry |
| `muninn.icns` (16–1024 px) | `scripts/icons.sh icns`, called by `scripts/macos-app.sh` | `Muninn.app` and the `.dmg` volume icon |
| any PNG | `scripts/icons.sh png SIZE OUT.png` | e.g. a 1024 px render for the Mac (below) |

Rendering needs `rsvg-convert` or ImageMagick with librsvg (alien-blue has both); packing
the `.ico` needs ImageMagick or Python Pillow. macOS has no SVG renderer that keeps the
transparent corners (Quick Look fills them white), so for `scripts/macos-app.sh` render a
PNG on Linux and pass it in:

```sh
scripts/icons.sh png 1024 dist/muninn-1024.png          # on Linux
ICON_PNG=dist/muninn-1024.png scripts/macos-app.sh       # on the Mac, after copying it over
```

The website's logo and favicons are separate files in `site/web/` (see `docs/website.md`).

## Replacing the logo

1. Save the new tile over `assets/icons/muninn.svg`: a square SVG (`viewBox="0 0 512 512"`)
   that renders well at 16 px.
2. Run `scripts/icons.sh check`, then `cargo test --test packaging`.
3. Rebuild the installers: `scripts/windows-setup.sh` (Linux, with the Windows host) and
   `scripts/macos-app.sh` (macOS). Commit only `muninn.svg`.

Released installers keep the icon they were built with, so a new logo ships with the
next release.
