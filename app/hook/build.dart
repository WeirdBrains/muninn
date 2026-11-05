// Native Assets build hook: compiles crates/muninn-ffi with cargo for the target platform
// and bundles the library with the app (no cargokit, no CocoaPods).
import 'package:flutter_rust_bridge_hooks/flutter_rust_bridge_hooks.dart';

void main(List<String> args) async {
  await build(args, (input, output) async {
    await const FlutterRustBridgeNativeAssetsBuilder(cratePath: '../crates/muninn-ffi').run(input: input, output: output);
  });
}
