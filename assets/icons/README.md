# Icons

`muninn.png` (1024×1024, Tokyo Night colours) is the app icon. `muninn.ico` (16–256 px) is
made from it for the Windows installer (`magick muninn.png -define icon:auto-resize=256,128,64,48,32,16 muninn.ico`);
`scripts/macos-app.sh` makes the macOS `.icns` from it with `sips` and `iconutil`.
