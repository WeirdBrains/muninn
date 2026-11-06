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

/// The theme in use. Restored from preferences.toml on launch and saved there on change
/// (see MuninnApp), the same `[theme]` the TUI and `mnnn theme` use.
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

/// True once the store is unlocked this session (the key lives in Rust memory only).
final unlockedProvider = StateProvider<bool>((ref) => false);

final overviewProvider = FutureProvider<Overview?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).overview();
});

/// The global time range: like `T` in the TUI it scopes every view.
final timeRangeProvider = StateProvider<TimeRange>((ref) => TimeRange.all);

/// The dataset shown in the Data view (by [Dataset.key]).
final selectedDatasetProvider = StateProvider<String?>((ref) => null);

/// Data view: chart or list (View ▸ Change view, like `Ctrl-w t`).
final dataAsChartProvider = StateProvider<bool>((ref) => true);

/// What the Data chart draws (TUI `L` / View ▸ Chart layers…). Defaults match the TUI.
class ChartLayers {
  const ChartLayers({
    this.candles = true,
    this.ohlc = false,
    this.line = false,
    this.area = false,
    this.volume = true,
    this.grid = true,
    this.crosshair = true,
    this.priceAxis = true,
    this.timeAxis = true,
  });
  final bool candles, ohlc, line, area, volume, grid, crosshair, priceAxis, timeAxis;

  ChartLayers copyWith({
    bool? candles,
    bool? ohlc,
    bool? line,
    bool? area,
    bool? volume,
    bool? grid,
    bool? crosshair,
    bool? priceAxis,
    bool? timeAxis,
  }) =>
      ChartLayers(
        candles: candles ?? this.candles,
        ohlc: ohlc ?? this.ohlc,
        line: line ?? this.line,
        area: area ?? this.area,
        volume: volume ?? this.volume,
        grid: grid ?? this.grid,
        crosshair: crosshair ?? this.crosshair,
        priceAxis: priceAxis ?? this.priceAxis,
        timeAxis: timeAxis ?? this.timeAxis,
      );

  @override
  bool operator ==(Object other) =>
      other is ChartLayers &&
      other.candles == candles &&
      other.ohlc == ohlc &&
      other.line == line &&
      other.area == area &&
      other.volume == volume &&
      other.grid == grid &&
      other.crosshair == crosshair &&
      other.priceAxis == priceAxis &&
      other.timeAxis == timeAxis;

  @override
  int get hashCode => Object.hash(candles, ohlc, line, area, volume, grid, crosshair, priceAxis, timeAxis);
}

final chartLayersProvider = StateProvider<ChartLayers>((ref) => const ChartLayers());

final selectedDatasetValueProvider = Provider<Dataset?>((ref) {
  final key = ref.watch(selectedDatasetProvider);
  final o = ref.watch(overviewProvider).value;
  if (key == null || o == null) return null;
  for (final d in o.datasets) {
    if (d.key == key) return d;
  }
  return null;
});

final barsProvider = FutureProvider<List<BarPoint>?>((ref) {
  final d = ref.watch(selectedDatasetValueProvider);
  final r = ref.watch(timeRangeProvider);
  if (d == null) return Future.value(null);
  return ref.watch(apiProvider).queryBars(d, r);
});

final selectedProjectProvider = StateProvider<Project?>((ref) => null);

final projectSeriesProvider = FutureProvider<List<ProjectSeries>?>((ref) {
  final p = ref.watch(selectedProjectProvider);
  if (p == null || !ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).projectSeries(p.id);
});

final netOverviewProvider = FutureProvider<NetOverview?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).netOverview();
});

final orgOverviewProvider = FutureProvider<OrgOverview?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).orgOverview();
});

/// `mnnn keys status` does not need the store password (the CLI does not either).
final keysStatusProvider = FutureProvider<KeysStatus>((ref) {
  return ref.watch(apiProvider).keysStatus();
});

/// The selected section (both the desktop menus and the mobile bottom bar drive it).
enum Section { home, data, projects, net, orgs, keys, help }

final sectionProvider = StateProvider<Section>((ref) => Section.home);
