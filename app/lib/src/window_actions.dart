/// Window control actions. Production uses the native window; tests inject a fake.
library;

import 'package:flutter_riverpod/flutter_riverpod.dart';

/// Minimize, maximize/restore, close and drag for the current desktop window.
abstract class WindowActions {
  const WindowActions();

  void minimize();
  void toggleMaximize();
  void close();
  void startDragging();
  bool get isMaximized;
}

/// No-op, used in widget tests and on platforms without a desktop window.
class NoopWindowActions implements WindowActions {
  const NoopWindowActions();

  @override
  void minimize() {}

  @override
  void toggleMaximize() {}

  @override
  void close() {}

  @override
  void startDragging() {}

  @override
  bool get isMaximized => false;
}

/// Records calls for widget tests.
class RecordingWindowActions implements WindowActions {
  RecordingWindowActions({bool maximized = false}) : isMaximized = maximized;

  final List<String> calls = [];

  @override
  bool isMaximized;

  @override
  void minimize() => calls.add('minimize');

  @override
  void toggleMaximize() {
    calls.add('toggleMaximize');
    isMaximized = !isMaximized;
  }

  @override
  void close() => calls.add('close');

  @override
  void startDragging() => calls.add('startDragging');
}

final windowActionsProvider = Provider<WindowActions>(
  (ref) => const NoopWindowActions(),
);
