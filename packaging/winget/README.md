# winget

Manifests for `Kommandosaldat.Muninn` (schema 1.9.0): a portable `mnnn.exe` inside the
release zip, for x64 and arm64. Windows binaries are cross-built on Linux with
llvm-mingw (`scripts/cross-windows.sh`, targets `*-pc-windows-gnullvm`).

On release, set `InstallerSha256` from `SHA256SUMS` (the release workflow does this),
then check and submit:

```sh
python3 packaging/winget/validate.py <winget-cli>/schemas/JSON/manifests/v1.9.0
winget validate --manifest packaging/winget/manifests/k/Kommandosaldat/Muninn/<ver>   # on Windows
```

Submit by PR to microsoft/winget-pkgs (e.g. `wingetcreate submit`).
