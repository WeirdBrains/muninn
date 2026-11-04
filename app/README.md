# Muninn app (Flutter)

Desktop and mobile app over `crates/muninn_ffi`. See [docs/flutter-app.md](../docs/flutter-app.md).

```
flutter pub get
flutter run -d linux
flutter analyze && flutter test
flutter_rust_bridge_codegen generate   # after changing crates/muninn-ffi/src/api
```

Publisher WeirdBrains · https://muninn.weirdbrains.com
