/// Riverpod state: the API, the active theme (name + light/dark), the theme list and data.
library;

import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/legacy.dart';

import 'api.dart';

/// Overridden in main() (FFI) and in tests (fake).
final apiProvider = Provider<MuninnApi>(
  (ref) => throw UnimplementedError('override apiProvider'),
);

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
  void toggleMode(Brightness platform) => state = ThemeSelection(
    state.name,
    state.variant(platform) == 'dark' ? 'light' : 'dark',
  );
}

final themeSelectionProvider =
    NotifierProvider<ThemeSelectionNotifier, ThemeSelection>(
      ThemeSelectionNotifier.new,
    );

final themeNamesProvider = FutureProvider<List<String>>(
  (ref) => ref.watch(apiProvider).listThemes(),
);

final platformBrightnessProvider = StateProvider<Brightness>(
  (ref) => Brightness.dark,
);

final resolvedThemeProvider = FutureProvider<MuninnTheme>((ref) {
  final sel = ref.watch(themeSelectionProvider);
  final b = ref.watch(platformBrightnessProvider);
  return ref.watch(apiProvider).loadTheme(sel.name, sel.variant(b));
});

final dataDirProvider = FutureProvider<DataDir>(
  (ref) => ref.watch(apiProvider).dataDir(),
);

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
    this.envelope = false,
    this.volume = true,
    this.ma = false,
    this.wicks = false,
    this.gaps = true,
    this.zeroVolume = false,
    this.grid = true,
    this.crosshair = true,
    this.priceAxis = true,
    this.timeAxis = true,
  });
  final bool candles,
      ohlc,
      line,
      area,
      envelope,
      volume,
      ma,
      wicks,
      gaps,
      zeroVolume,
      grid,
      crosshair,
      priceAxis,
      timeAxis;

  ChartLayers copyWith({
    bool? candles,
    bool? ohlc,
    bool? line,
    bool? area,
    bool? envelope,
    bool? volume,
    bool? ma,
    bool? wicks,
    bool? gaps,
    bool? zeroVolume,
    bool? grid,
    bool? crosshair,
    bool? priceAxis,
    bool? timeAxis,
  }) => ChartLayers(
    candles: candles ?? this.candles,
    ohlc: ohlc ?? this.ohlc,
    line: line ?? this.line,
    area: area ?? this.area,
    envelope: envelope ?? this.envelope,
    volume: volume ?? this.volume,
    ma: ma ?? this.ma,
    wicks: wicks ?? this.wicks,
    gaps: gaps ?? this.gaps,
    zeroVolume: zeroVolume ?? this.zeroVolume,
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
      other.envelope == envelope &&
      other.volume == volume &&
      other.ma == ma &&
      other.wicks == wicks &&
      other.gaps == gaps &&
      other.zeroVolume == zeroVolume &&
      other.grid == grid &&
      other.crosshair == crosshair &&
      other.priceAxis == priceAxis &&
      other.timeAxis == timeAxis;

  @override
  int get hashCode => Object.hash(
    candles,
    ohlc,
    line,
    area,
    envelope,
    volume,
    ma,
    wicks,
    gaps,
    zeroVolume,
    grid,
    crosshair,
    priceAxis,
    timeAxis,
  );

  Map<String, dynamic> toJson() => {
    'candles': candles,
    'ohlc': ohlc,
    'line': line,
    'area': area,
    'envelope': envelope,
    'volume': volume,
    'ma': ma,
    'wicks': wicks,
    'gaps': gaps,
    'zeroVolume': zeroVolume,
    'grid': grid,
    'crosshair': crosshair,
    'priceAxis': priceAxis,
    'timeAxis': timeAxis,
  };

  factory ChartLayers.fromJson(Map<String, dynamic>? j) {
    if (j == null) return const ChartLayers();
    bool g(String k, bool d) => j[k] as bool? ?? d;
    return ChartLayers(
      candles: g('candles', true),
      ohlc: g('ohlc', false),
      line: g('line', false),
      area: g('area', false),
      envelope: g('envelope', false),
      volume: g('volume', true),
      ma: g('ma', false),
      wicks: g('wicks', false),
      gaps: g('gaps', true),
      zeroVolume: g('zeroVolume', false),
      grid: g('grid', true),
      crosshair: g('crosshair', true),
      priceAxis: g('priceAxis', true),
      timeAxis: g('timeAxis', true),
    );
  }
}

final chartLayersProvider = StateProvider<ChartLayers>(
  (ref) => const ChartLayers(),
);

/// TUI line metric (`N` on a chart tile). VWAP per bar is the typical price.
enum ChartLineMetric { open, high, low, close, typical, median, ohlcAvg, vwap }

extension ChartLineMetricX on ChartLineMetric {
  String get label => switch (this) {
    ChartLineMetric.open => 'open',
    ChartLineMetric.high => 'high',
    ChartLineMetric.low => 'low',
    ChartLineMetric.close => 'close',
    ChartLineMetric.typical => 'typical',
    ChartLineMetric.median => 'median',
    ChartLineMetric.ohlcAvg => 'ohlc avg',
    ChartLineMetric.vwap => 'vwap',
  };

  double value(BarPoint b) => switch (this) {
    ChartLineMetric.open => b.open,
    ChartLineMetric.high => b.high,
    ChartLineMetric.low => b.low,
    ChartLineMetric.close => b.close,
    ChartLineMetric.typical ||
    ChartLineMetric.vwap => (b.high + b.low + b.close) / 3,
    ChartLineMetric.median => (b.high + b.low) / 2,
    ChartLineMetric.ohlcAvg => (b.open + b.high + b.low + b.close) / 4,
  };

  ChartLineMetric get next {
    const all = ChartLineMetric.values;
    return all[(index + 1) % all.length];
  }
}

final chartLineMetricProvider = StateProvider<ChartLineMetric>(
  (ref) => ChartLineMetric.close,
);

/// A TUI moving average (`sma20`, `ema50`). Period is 2..1000.
class ChartMa {
  const ChartMa({required this.ema, required this.period});
  final bool ema;
  final int period;
  String get label => '${ema ? 'ema' : 'sma'}$period';

  @override
  bool operator ==(Object other) =>
      other is ChartMa && other.ema == ema && other.period == period;

  @override
  int get hashCode => Object.hash(ema, period);
}

/// TUI default: `sma20,sma50`.
const defaultChartMas = [
  ChartMa(ema: false, period: 20),
  ChartMa(ema: false, period: 50),
];

final chartMasProvider = StateProvider<List<ChartMa>>((ref) => defaultChartMas);

/// Parse `sma20, ema50` the way the TUI `M` prompt does.
List<ChartMa> parseChartMas(String s) {
  final out = <ChartMa>[];
  for (final raw in s.split(RegExp(r'[,\s]+'))) {
    final p = raw.trim().toLowerCase();
    if (p.isEmpty) continue;
    var ema = false;
    var n = p;
    if (p.startsWith('ema')) {
      ema = true;
      n = p.substring(3);
    } else if (p.startsWith('sma')) {
      n = p.substring(3);
    }
    final period = int.tryParse(n);
    if (period == null) {
      throw FormatException('bad moving average `$p` (use sma20, ema50)');
    }
    if (period < 2 || period > 1000) {
      throw FormatException('period $period out of range 2..1000');
    }
    out.add(ChartMa(ema: ema, period: period));
  }
  if (out.isEmpty) {
    throw const FormatException(
      'give at least one moving average (sma20, ema50)',
    );
  }
  return out;
}

/// Visible slice of the already-loaded bars (pinch, wheel, or `+`/`-` / `h`/`l`).
class ChartViewport {
  const ChartViewport({this.scale = 1, this.start = 0});
  final double scale;
  final int start;
  static const maxScale = 24.0;

  int visibleCount(int n) {
    if (n <= 0) return 0;
    if (scale <= 1.001) return n;
    final c = (n / scale).round();
    return c < 1 ? 1 : c;
  }

  int visibleStart(int n) {
    if (n <= 0) return 0;
    final maxStart = n - visibleCount(n);
    if (start < 0 || maxStart < 0) return 0;
    return start > maxStart ? maxStart : start;
  }

  bool zoomed(int n) => scale > 1.001 && visibleCount(n) < n;

  ChartViewport clampTo(int n) =>
      ChartViewport(scale: scale, start: visibleStart(n));

  ChartViewport zoomAt(double newScale, {required int n, double frac = 0.5}) {
    if (n <= 0) return this;
    if (newScale < 1) newScale = 1;
    if (newScale > maxScale) newScale = maxScale;
    final oldCount = visibleCount(n);
    final oldStart = visibleStart(n);
    final clampedFrac = frac < 0 ? 0.0 : (frac > 1 ? 1.0 : frac);
    final focalIndex = oldStart + clampedFrac * oldCount;
    final next = ChartViewport(scale: newScale, start: start);
    final newCount = next.visibleCount(n);
    final newStart = (focalIndex - clampedFrac * newCount).round();
    return ChartViewport(scale: newScale, start: newStart).clampTo(n);
  }

  ChartViewport panBy(int delta, int n) =>
      ChartViewport(scale: scale, start: visibleStart(n) + delta).clampTo(n);

  /// Crop the visible window so it starts at [index], keeping the current end
  /// (TUI `[` `{` / `chart_set_start`). No-op when [index] is not before the end.
  ChartViewport periodStart(int index, int n) {
    if (n <= 0) return this;
    final end = (visibleStart(n) + visibleCount(n) - 1).clamp(0, n - 1);
    if (index < 0 || index >= end) return this;
    final vis = end - index + 1;
    final newScale = (n / vis).clamp(1.0, maxScale);
    return ChartViewport(scale: newScale, start: index).clampTo(n);
  }

  /// Crop the visible window so it ends at [index], keeping the current start
  /// (TUI `]` `}` / `chart_set_end`).
  ChartViewport periodEnd(int index, int n) {
    if (n <= 0) return this;
    final start = visibleStart(n);
    if (index <= start || index >= n) return this;
    final vis = index - start + 1;
    final newScale = (n / vis).clamp(1.0, maxScale);
    return ChartViewport(scale: newScale, start: start).clampTo(n);
  }

  /// Centre the current window on [index] (TUI `@` / jump to date). At scale 1
  /// the window already shows every loaded bar, so this is a no-op.
  ChartViewport jumpTo(int index, int n) {
    if (n <= 0) return this;
    final vis = visibleCount(n);
    return ChartViewport(scale: scale, start: index - vis ~/ 2).clampTo(n);
  }

  @override
  bool operator ==(Object other) =>
      other is ChartViewport && other.scale == scale && other.start == start;

  @override
  int get hashCode => Object.hash(scale, start);

  Map<String, dynamic> toJson() => {'scale': scale, 'start': start};

  factory ChartViewport.fromParts({double? scale, int? start}) =>
      ChartViewport(scale: scale ?? 1, start: start ?? 0);
}

final chartViewportProvider = StateProvider<ChartViewport>(
  (ref) => const ChartViewport(),
);

/// Index of the loaded bar whose timestamp is closest to [ts].
int nearestBarIndex(List<BarPoint> bars, int ts) {
  if (bars.isEmpty) return 0;
  var best = 0;
  var bestAbs = (bars[0].ts - ts).abs();
  for (var i = 1; i < bars.length; i++) {
    final d = (bars[i].ts - ts).abs();
    if (d < bestAbs) {
      best = i;
      bestAbs = d;
    }
  }
  return best;
}

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

/// Projects window pages (TUI Projects / Journals / Searches / Reports / Code / Pipes / Pipelines / Rules / Reviews / Notifications).
enum ProjectPane {
  projects,
  journals,
  searches,
  reports,
  code,
  pipes,
  pipelines,
  rules,
  reviews,
  notifications,
}

final projectPaneProvider = StateProvider<ProjectPane>(
  (ref) => ProjectPane.projects,
);

final projectItemsProvider = FutureProvider<List<ProjectItemInfo>?>((ref) {
  final p = ref.watch(selectedProjectProvider);
  if (p == null || !ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).projectItems(p.id);
});

final journalsProvider = FutureProvider<List<JournalInfo>?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).journals();
});

final selectedJournalProvider = StateProvider<JournalInfo?>((ref) => null);

final journalEntriesProvider = FutureProvider<List<JournalEntryInfo>?>((ref) {
  final j = ref.watch(selectedJournalProvider);
  if (j == null || !ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).journalEntries(j.id);
});

final journalNotesProvider = FutureProvider<List<JournalNoteInfo>?>((ref) {
  final j = ref.watch(selectedJournalProvider);
  if (j == null || !ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).journalNotes(j.id);
});

final searchesProvider = FutureProvider<List<SearchInfo>?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).searches();
});

final reportsProvider = FutureProvider<List<ReportInfo>?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).reports();
});

final selectedReportProvider = StateProvider<ReportInfo?>((ref) => null);

final reportSectionsProvider = FutureProvider<List<ReportSectionInfo>?>((ref) {
  final r = ref.watch(selectedReportProvider);
  if (r == null || !ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).reportSections(r.id);
});

final codeSpacesProvider = FutureProvider<List<CodeSpaceInfo>?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).codeSpaces();
});

final pipesLiveProvider = FutureProvider<List<LivePipeInfo>?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).pipesLive();
});

final pipelinesProvider = FutureProvider<List<PipelineInfo>?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).pipelines();
});

final rulesProvider = FutureProvider<List<RuleInfo>?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).rules();
});

final reviewsProvider = FutureProvider<List<ReviewInfo>?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).reviews();
});

final reviewsJoinedProvider = FutureProvider<List<ReviewingInfo>?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).reviewsJoined();
});

final notificationsUnreadOnlyProvider = StateProvider<bool>((ref) => false);

final notificationsProvider = FutureProvider<List<NotificationInfo>?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  final unread = ref.watch(notificationsUnreadOnlyProvider);
  return ref.watch(apiProvider).notifications(unread: unread);
});

void refreshProjectExtras(WidgetRef ref) {
  ref.invalidate(overviewProvider);
  ref.invalidate(projectSeriesProvider);
  ref.invalidate(projectItemsProvider);
  ref.invalidate(journalsProvider);
  ref.invalidate(journalEntriesProvider);
  ref.invalidate(journalNotesProvider);
  ref.invalidate(searchesProvider);
  ref.invalidate(reportsProvider);
  ref.invalidate(reportSectionsProvider);
  ref.invalidate(codeSpacesProvider);
  ref.invalidate(pipesLiveProvider);
  ref.invalidate(pipelinesProvider);
  ref.invalidate(rulesProvider);
  ref.invalidate(reviewsProvider);
  ref.invalidate(reviewsJoinedProvider);
  ref.invalidate(notificationsProvider);
}

final netOverviewProvider = FutureProvider<NetOverview?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).netOverview();
});

/// Net window pages (TUI Node / Peers / Relays / Invites / Shared plus leftovers).
enum NetPane {
  node,
  peers,
  relays,
  invites,
  shared,
  settings,
  connections,
  exchanges,
  search,
  shards,
  mcp,
  collab,
}

final netPaneProvider = StateProvider<NetPane>((ref) => NetPane.node);

final netInvitesProvider = FutureProvider<NetInvites?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).netInvites();
});

final netRelaysProvider = FutureProvider<NetRelayInfo?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).netRelays();
});

final netSharedProvider = FutureProvider<List<NetShareInfo>?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).netShared();
});

final netConfigProvider = FutureProvider<NetConfig?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).netConfig();
});

final netConnectionsProvider = FutureProvider<NetConnections?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).netConnections();
});

final netExchangesProvider = FutureProvider<List<NetExchangeInfo>?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).netExchanges();
});

final netShardsProvider = FutureProvider<NetShards?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).netShards();
});

final mcpServersProvider = FutureProvider<List<McpServerInfo>?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).mcpServers();
});

final mcpClientsProvider = FutureProvider<List<McpClientInfo>?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).mcpClients();
});

final peerMessagesProvider = FutureProvider<List<PeerMessageInfo>?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).peerMessages();
});

final liveSessionsProvider = FutureProvider<List<LiveSessionInfo>?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).liveSessions();
});

void refreshNet(WidgetRef ref) {
  ref.invalidate(netOverviewProvider);
  ref.invalidate(netInvitesProvider);
  ref.invalidate(netRelaysProvider);
  ref.invalidate(netSharedProvider);
  ref.invalidate(netConfigProvider);
  ref.invalidate(netConnectionsProvider);
  ref.invalidate(netExchangesProvider);
  ref.invalidate(netShardsProvider);
  ref.invalidate(mcpServersProvider);
  ref.invalidate(mcpClientsProvider);
  ref.invalidate(peerMessagesProvider);
  ref.invalidate(liveSessionsProvider);
}

final orgOverviewProvider = FutureProvider<OrgOverview?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).orgOverview();
});

final orgHostStatusProvider = FutureProvider<OrgHostStatus?>((ref) {
  if (!ref.watch(unlockedProvider)) return Future.value(null);
  return ref.watch(apiProvider).orgHostStatus();
});

void refreshOrgs(WidgetRef ref) {
  ref.invalidate(orgOverviewProvider);
  ref.invalidate(orgHostStatusProvider);
}

/// `mnnn keys status` does not need the store password (the CLI does not either).
final keysStatusProvider = FutureProvider<KeysStatus>((ref) {
  return ref.watch(apiProvider).keysStatus();
});

/// Help topic id (`mnnn help <id>` / TUI Help). `?` and F1 set this from the current section.
final helpTopicIdProvider = StateProvider<String>((ref) => 'flutter-app');
final helpSearchQueryProvider = StateProvider<String>((ref) => '');
final helpSearchOpenProvider = StateProvider<bool>((ref) => false);

/// The selected section (both the desktop menus and the mobile bottom bar drive it).
enum Section { home, data, projects, net, orgs, keys, help }

final sectionProvider = StateProvider<Section>((ref) => Section.home);
