/// Policy for custom window controls on Flutter desktop apps.
///
/// The logic is pure and testable: [WindowChromePolicy.resolve] takes a
/// [TargetPlatform] and an environment map (normally [Platform.environment]).
library;

import 'dart:io' show Platform;

import 'package:flutter/foundation.dart'
    show TargetPlatform, defaultTargetPlatform, kIsWeb;
import 'package:flutter_riverpod/flutter_riverpod.dart';

/// Which side of the title bar the window controls sit on.
enum WindowControlSide { left, right }

/// Which custom window controls to show, and where.
class WindowChromePolicy {
  const WindowChromePolicy({
    required this.enabled,
    required this.side,
    required this.showMinimize,
    required this.showMaximize,
    required this.showClose,
    required this.allowDrag,
    required this.doubleClickMaximizes,
  });

  /// No custom chrome (mobile, or widget tests that have not opted in).
  static const disabled = WindowChromePolicy(
    enabled: false,
    side: WindowControlSide.right,
    showMinimize: false,
    showMaximize: false,
    showClose: false,
    allowDrag: false,
    doubleClickMaximizes: false,
  );

  final bool enabled;
  final WindowControlSide side;
  final bool showMinimize;
  final bool showMaximize;
  final bool showClose;
  final bool allowDrag;
  final bool doubleClickMaximizes;

  /// Desktop platforms get a frameless window with custom controls.
  ///
  /// On macOS the controls sit on the left in traffic-light order (close,
  /// minimize, maximize). On Windows and Linux they sit on the right
  /// (minimize, maximize, close). Under a Linux tiling window manager only
  /// close is shown, and double-click does not maximize.
  factory WindowChromePolicy.resolve({
    required TargetPlatform platform,
    Map<String, String> environment = const {},
  }) {
    final desktop =
        platform == TargetPlatform.linux ||
        platform == TargetPlatform.windows ||
        platform == TargetPlatform.macOS;
    if (!desktop) return WindowChromePolicy.disabled;

    final tiling =
        platform == TargetPlatform.linux &&
        isLinuxTilingWindowManager(environment);
    final mac = platform == TargetPlatform.macOS;
    return WindowChromePolicy(
      enabled: true,
      side: mac ? WindowControlSide.left : WindowControlSide.right,
      showMinimize: !tiling,
      showMaximize: !tiling,
      showClose: true,
      allowDrag: true,
      doubleClickMaximizes: !tiling,
    );
  }
}

/// Known tiling window managers, matched against desktop/session names.
const linuxTilingDesktops = {
  'hyprland',
  'sway',
  'i3',
  'i3-gaps',
  'awesome',
  'bspwm',
  'dwm',
  'qtile',
  'xmonad',
  'herbstluftwm',
  'river',
  'niri',
  'leftwm',
  'spectrwm',
  'notion',
  'wmii',
  'newm',
};

/// True when [environment] indicates a Linux tiling compositor or WM.
///
/// Checks compositor-specific variables first (`HYPRLAND_INSTANCE_SIGNATURE`,
/// `SWAYSOCK`, `I3SOCK`), then colon- or space-separated names in
/// `XDG_CURRENT_DESKTOP`, `XDG_SESSION_DESKTOP` and `DESKTOP_SESSION`.
bool isLinuxTilingWindowManager(Map<String, String> environment) {
  if (environment['HYPRLAND_INSTANCE_SIGNATURE']?.isNotEmpty == true) {
    return true;
  }
  if (environment['SWAYSOCK']?.isNotEmpty == true) return true;
  if (environment['I3SOCK']?.isNotEmpty == true) return true;

  const keys = [
    'XDG_CURRENT_DESKTOP',
    'XDG_SESSION_DESKTOP',
    'DESKTOP_SESSION',
  ];
  for (final key in keys) {
    final raw = environment[key];
    if (raw == null || raw.isEmpty) continue;
    for (final part in raw.split(RegExp(r'[:\s]'))) {
      final name = part.trim().toLowerCase();
      if (name.isEmpty) continue;
      if (linuxTilingDesktops.contains(name)) return true;
    }
  }
  return false;
}

/// Opt-in for drawing custom window controls. Production desktop [main]
/// overrides this to true; widget tests leave it false so existing pumps
/// keep their system-chrome-free layout without talking to the window plugin.
final windowChromeEnabledProvider = Provider<bool>((ref) => false);

/// Resolved chrome policy. Disabled unless [windowChromeEnabledProvider] is
/// true, then derived from the platform and process environment.
final windowChromePolicyProvider = Provider<WindowChromePolicy>((ref) {
  if (!ref.watch(windowChromeEnabledProvider)) {
    return WindowChromePolicy.disabled;
  }
  return WindowChromePolicy.resolve(
    platform: defaultTargetPlatform,
    environment: kIsWeb ? const {} : Platform.environment,
  );
});
