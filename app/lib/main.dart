import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'src/app.dart';
import 'src/ffi_api.dart';
import 'src/rust/frb_generated.dart';
import 'src/state.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  // Desktop shares the TUI's data dir (MUNINN_HOME, else ~/.muninn). Mobile keeps its own
  // app-sandboxed copy, synced with a desktop node in a later phase.
  final home = Platform.environment['MUNINN_HOME'];
  runApp(ProviderScope(
    overrides: [apiProvider.overrideWithValue(FfiMuninnApi(home: home))],
    child: const MuninnApp(),
  ));
}
