/// Desktop-only wiring for [window_manager] (Linux, Windows, macOS).
library;

import 'package:flutter/widgets.dart';
import 'package:window_manager/window_manager.dart';

import 'window_actions.dart';

/// Hides the system title bar and native window buttons.
///
/// Safe to call more than once. If the native window is not ready yet, it
/// retries on the next frame.
void configureDesktopWindow() {
  final window = WindowManager.instance.getCurrent();
  if (window == null) {
    WidgetsBinding.instance.addPostFrameCallback(
      (_) => configureDesktopWindow(),
    );
    return;
  }
  window.title = 'Muninn';
  window.titleBarStyle = TitleBarStyle.hidden;
  window.isWindowControlButtonsVisible = false;
}

/// Forwards custom control clicks to the current [Window].
class DesktopWindowActions implements WindowActions {
  const DesktopWindowActions();

  Window? get _window => WindowManager.instance.getCurrent();

  @override
  void minimize() => _window?.minimize();

  @override
  void toggleMaximize() {
    final window = _window;
    if (window == null) return;
    if (window.isMaximized) {
      window.unmaximize();
    } else {
      window.maximize();
    }
  }

  @override
  void close() => _window?.close();

  @override
  void startDragging() => _window?.startDragging();

  @override
  bool get isMaximized => _window?.isMaximized ?? false;
}
