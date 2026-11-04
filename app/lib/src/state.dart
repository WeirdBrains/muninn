/// Riverpod state: the API, the active theme (name + light/dark), the theme list and data.
library;

import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/legacy.dart';

import 'api.dart';

/// Overridden in main() (FFI) and in tests (fake).
final apiProvider = Provider<MuninnApi>((ref) => throw UnimplementedError('override apiProvider'));

class ThemeSelection {
  const ThemeSelection(this.name, this.mode);
  final String name;
  /// `light`, `dark` or `system` (same as `mnnn theme mode`).
  final String mode;

  String variant(Brightness platform) => switch (mode) {
        'light' => 'light',
        'dark' => 'dark',
        _ => platform == Brightness.light ? 'light' : 'dark',
      };
}

/// The theme in use. Starts from the TUI's preferences; switching here is for this session
/// (the app never rewrites preferences.toml in this phase).
class ThemeSelectionNotifier extends Notifier<ThemeSelection> {
  @override
  ThemeSelection build() => const ThemeSelection('tokyo-night', 'system');

  void set(String name) => state = ThemeSelection(name, state.mode);
  void setMode(String mode) => state = ThemeSelection(state.name, mode);
  void toggleMode(Brightness platform) =>
      state = ThemeSelection(state.name, state.variant(platform) == 'dark' ? 'light' : 'dark');
}

final themeSelectionProvider = NotifierProvider<ThemeSelectionNotifier, ThemeSelection>(ThemeSelectionNotifier.new);

final themeNamesProvider = FutureProvider<List<String>>((ref) => ref.watch(apiProvider).listThemes());

final platformBrightnessProvider = StateProvider<Brightness>((ref) => Brightness.dark);

final resolvedThemeProvider = FutureProvider<MuninnTheme>((ref) {
  final sel = ref.watch(themeSelectionProvider);
  final b = ref.watch(platformBrightnessProvider);
  return ref.watch(apiProvider).loadTheme(sel.name, sel.variant(b));
});

final dataDirProvider = FutureProvider<DataDir>((ref) => ref.watch(apiProvider).dataDir());

/// The password the user typed this session (memory only, never stored).
final passwordProvider = StateProvider<String?>((ref) => null);

final overviewProvider = FutureProvider<Overview?>((ref) {
  final pw = ref.watch(passwordProvider);
  if (pw == null) return Future.value(null);
  return ref.watch(apiProvider).overview(pw);
});

/// The selected section (both the desktop menus and the mobile bottom bar drive it).
enum Section { home, data, projects, net, orgs, keys, help }

final sectionProvider = StateProvider<Section>((ref) => Section.home);
