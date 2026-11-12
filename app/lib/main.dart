import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'src/app.dart';
import 'src/ffi_api.dart';
import 'src/rust/frb_generated.dart';
import 'src/state.dart';
import 'src/window_actions.dart';
import 'src/window_chrome.dart';
import 'src/window_desktop.dart';

bool get _isDesktop =>
    Platform.isLinux || Platform.isWindows || Platform.isMacOS;

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  // Desktop shares the TUI's data dir (MUNINN_HOME, else ~/.muninn). Mobile keeps its own
  // app-sandboxed copy, synced with a desktop node in a later phase.
  final home = Platform.environment['MUNINN_HOME'];
  if (_isDesktop) configureDesktopWindow();
  runApp(
    ProviderScope(
      overrides: [
        apiProvider.overrideWithValue(FfiMuninnApi(home: home)),
        if (_isDesktop) windowChromeEnabledProvider.overrideWithValue(true),
        if (_isDesktop)
          windowActionsProvider.overrideWithValue(const DesktopWindowActions()),
      ],
      child: const MuninnApp(),
    ),
  );
}
