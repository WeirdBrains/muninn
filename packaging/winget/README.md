# winget

Manifests for `WeirdBrains.muninn.tui` (schema 1.9.0): the per-user Inno Setup
installer (`muninn-<ver>-<arch>-windows-setup.exe`, see `packaging/windows/`), for
x64 and arm64. Package name stays **Muninn**; this id is the TUI/CLI (`mnnn`),
separate from the Flutter app's MSIX/Store identity `WeirdBrains.Muninn`.

`scripts/weirdnet_release.py` (the `move` entry in `release.toml`) moves the template to the new version; `scripts/fill-sums.py`
writes the filled-in manifests to
`<dist>/packaging/winget/manifests/w/WeirdBrains/muninn/tui/<ver>`.
Check and submit:

```sh
python3 packaging/winget/validate.py <winget-cli>/schemas/JSON/manifests/v1.9.0
winget validate --manifest <dist>\packaging\winget\manifests\w\WeirdBrains\muninn\tui\<ver>   # on Windows
```

Submit by PR to microsoft/winget-pkgs: copy that folder to the same path in a fork and
open a PR (or `wingetcreate submit <folder>`).
