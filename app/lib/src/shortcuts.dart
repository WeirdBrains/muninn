/// TUI keybinding parity: the same keys run the same named actions the app has.
/// Ignored while a text field is focused. See docs/flutter-app.md for platform exceptions.
library;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/legacy.dart';

import 'api.dart';
import 'io_dialogs.dart';
import 'state.dart';
import 'time_range.dart';
import 'views/chart.dart';
import 'views/help.dart';
import 'views/projects.dart';

/// F10 asks the desktop menu bar to open (File, the first menu).
final menuBarRequestProvider = StateProvider<int>((ref) => 0);

void requestOpenMenuBar(WidgetRef ref) =>
    ref.read(menuBarRequestProvider.notifier).state =
        ref.read(menuBarRequestProvider) + 1;

/// Re-read overview, net, orgs, keys, bars and project series (TUI F5 / Data ▸ Refresh).
void refreshMuninn(WidgetRef ref) {
  ref.invalidate(overviewProvider);
  ref.invalidate(netOverviewProvider);
  ref.invalidate(orgOverviewProvider);
  ref.invalidate(keysStatusProvider);
  ref.invalidate(barsProvider);
  ref.invalidate(projectSeriesProvider);
}

bool isEditingText() {
  final ctx = FocusManager.instance.primaryFocus?.context;
  if (ctx == null) return false;
  return ctx.widget is EditableText ||
      ctx.findAncestorWidgetOfExactType<EditableText>() != null;
}

bool get isDesktopShell {
  if (kIsWeb) return false;
  switch (defaultTargetPlatform) {
    case TargetPlatform.linux:
    case TargetPlatform.macOS:
    case TargetPlatform.windows:
      return true;
    default:
      return false;
  }
}

void toggleDataView(WidgetRef ref) {
  ref.read(dataAsChartProvider.notifier).state = !ref.read(dataAsChartProvider);
}

/// Wraps the shell so TUI keys work on desktop and with a hardware keyboard on a phone.
class MuninnShortcuts extends ConsumerStatefulWidget {
  const MuninnShortcuts({super.key, required this.child});
  final Widget child;
  @override
  ConsumerState<MuninnShortcuts> createState() => _MuninnShortcutsState();
}

class _MuninnShortcutsState extends ConsumerState<MuninnShortcuts> {
  bool _afterCtrlW = false;

  @override
  Widget build(BuildContext context) => Focus(
    autofocus: true,
    onKeyEvent: (node, e) => _onKey(context, e),
    child: widget.child,
  );

  KeyEventResult _onKey(BuildContext context, KeyEvent e) {
    final down = e is KeyDownEvent;
    final repeat = e is KeyRepeatEvent;
    if (!down && !repeat) return KeyEventResult.ignored;
    if (isEditingText()) {
      _afterCtrlW = false;
      return KeyEventResult.ignored;
    }

    final keys = HardwareKeyboard.instance;
    final ctrl = keys.isControlPressed;
    final alt = keys.isAltPressed;
    final meta = keys.isMetaPressed;
    final shift = keys.isShiftPressed;
    final ch = e.character;
    final key = e.logicalKey;

    if (down && ctrl && !alt && !meta && key == LogicalKeyboardKey.keyW) {
      _afterCtrlW = true;
      return KeyEventResult.handled;
    }
    if (_afterCtrlW) {
      _afterCtrlW = false;
      if (down && !shift && !alt && !meta && key == LogicalKeyboardKey.keyT) {
        toggleDataView(ref);
        return KeyEventResult.handled;
      }
      // Other Ctrl-w tiling keys have no tile actions in the app; do not invent bindings.
    }

    if (repeat) {
      if (_chartShowing && !ctrl && !alt && !meta) {
        if (_zoomOrPan(key, ch, shift)) return KeyEventResult.handled;
      }
      return KeyEventResult.ignored;
    }

    // Function keys and help (no letter modifiers required).
    if (!ctrl && !alt && !meta) {
      if (key == LogicalKeyboardKey.f5) {
        refreshMuninn(ref);
        return KeyEventResult.handled;
      }
      if (key == LogicalKeyboardKey.f1 ||
          ch == '?' ||
          key == LogicalKeyboardKey.question ||
          (key == LogicalKeyboardKey.slash && shift)) {
        openHelp(ref);
        return KeyEventResult.handled;
      }
      if (key == LogicalKeyboardKey.f10) {
        requestOpenMenuBar(ref);
        return KeyEventResult.handled;
      }
    }

    if (ctrl || alt || meta) return KeyEventResult.ignored;

    // Shift+letter menu shortcuts (T, X, P, K, G, L, W) — same characters as the TUI.
    if (ch == 'T' || (key == LogicalKeyboardKey.keyT && shift)) {
      showTimeRangePicker(context, ref);
      return KeyEventResult.handled;
    }
    if (ch == 'X' || (key == LogicalKeyboardKey.keyX && shift)) {
      ref.read(timeRangeProvider.notifier).state = TimeRange.all;
      return KeyEventResult.handled;
    }
    if (ch == 'P' || (key == LogicalKeyboardKey.keyP && shift)) {
      ref.read(sectionProvider.notifier).state = Section.projects;
      return KeyEventResult.handled;
    }
    if (ch == 'K' || (key == LogicalKeyboardKey.keyK && shift)) {
      ref.read(sectionProvider.notifier).state = Section.net;
      return KeyEventResult.handled;
    }
    if (ch == 'W' || (key == LogicalKeyboardKey.keyW && shift)) {
      ref.read(sectionProvider.notifier).state = Section.net;
      return KeyEventResult.handled;
    }
    if (ch == 'G' || (key == LogicalKeyboardKey.keyG && shift)) {
      ref.read(sectionProvider.notifier).state = Section.orgs;
      return KeyEventResult.handled;
    }
    if (ch == 'L' || (key == LogicalKeyboardKey.keyL && shift)) {
      showChartLayersSheet(context, ref);
      return KeyEventResult.handled;
    }

    // File ▸ Import… (`i`); File ▸ Quit (`q`) on desktop only.
    if (ch == 'i' || (key == LogicalKeyboardKey.keyI && !shift)) {
      showImportDialog(context, ref);
      return KeyEventResult.handled;
    }
    if ((ch == 'q' || (key == LogicalKeyboardKey.keyQ && !shift)) &&
        isDesktopShell) {
      SystemNavigator.pop();
      return KeyEventResult.handled;
    }

    // Projects window: `n` new (TUI inside Projects).
    if (ref.read(sectionProvider) == Section.projects &&
        (ch == 'n' || (key == LogicalKeyboardKey.keyN && !shift))) {
      showNewProjectDialog(context, ref);
      return KeyEventResult.handled;
    }

    // Data list or chart: `,` / `.` previous / next series (TUI list and chart tiles).
    if (ref.read(sectionProvider) == Section.data) {
      if (ch == ',' || key == LogicalKeyboardKey.comma) {
        _selectAdjacentDataset(-1);
        return KeyEventResult.handled;
      }
      if (ch == '.' || key == LogicalKeyboardKey.period) {
        _selectAdjacentDataset(1);
        return KeyEventResult.handled;
      }
      // Data tile `t`: chart ↔ table when the list is showing.
      if (!_chartShowing &&
          (ch == 't' || (key == LogicalKeyboardKey.keyT && !shift))) {
        toggleDataView(ref);
        return KeyEventResult.handled;
      }
    }

    if (_chartShowing) {
      if (ch == '{' ||
          key == LogicalKeyboardKey.braceLeft ||
          (key == LogicalKeyboardKey.bracketLeft && shift)) {
        _pickChartPeriod(context, start: true, typed: true);
        return KeyEventResult.handled;
      }
      if (ch == '}' ||
          key == LogicalKeyboardKey.braceRight ||
          (key == LogicalKeyboardKey.bracketRight && shift)) {
        _pickChartPeriod(context, start: false, typed: true);
        return KeyEventResult.handled;
      }
      if (ch == '[' || key == LogicalKeyboardKey.bracketLeft) {
        _pickChartPeriod(context, start: true, typed: false);
        return KeyEventResult.handled;
      }
      if (ch == ']' || key == LogicalKeyboardKey.bracketRight) {
        _pickChartPeriod(context, start: false, typed: false);
        return KeyEventResult.handled;
      }
      if (ch == '@' || key == LogicalKeyboardKey.at) {
        _jumpChartDate(context);
        return KeyEventResult.handled;
      }
      if (ch == 'M' || (key == LogicalKeyboardKey.keyM && shift)) {
        showMovingAveragesDialog(context, ref);
        return KeyEventResult.handled;
      }
      if (ch == 'N' || (key == LogicalKeyboardKey.keyN && shift)) {
        final cur = ref.read(chartLineMetricProvider);
        ref.read(chartLineMetricProvider.notifier).state = cur.next;
        return KeyEventResult.handled;
      }
      if (_toggleLayer(ch, key, shift)) return KeyEventResult.handled;
      if (_zoomOrPan(key, ch, shift)) return KeyEventResult.handled;
    }

    return KeyEventResult.ignored;
  }

  bool get _chartShowing =>
      ref.read(sectionProvider) == Section.data &&
      ref.read(dataAsChartProvider);

  bool _toggleLayer(String? ch, LogicalKeyboardKey key, bool shift) {
    ChartLayers Function(ChartLayers)? next;
    if (ch == 'c' || (key == LogicalKeyboardKey.keyC && !shift)) {
      next = (l) => l.copyWith(candles: !l.candles);
    } else if (ch == 'b' || (key == LogicalKeyboardKey.keyB && !shift)) {
      next = (l) => l.copyWith(ohlc: !l.ohlc);
    } else if (ch == 'n' || (key == LogicalKeyboardKey.keyN && !shift)) {
      next = (l) => l.copyWith(line: !l.line);
    } else if (ch == 'a' || (key == LogicalKeyboardKey.keyA && !shift)) {
      next = (l) => l.copyWith(area: !l.area);
    } else if (ch == 'e' || (key == LogicalKeyboardKey.keyE && !shift)) {
      next = (l) => l.copyWith(envelope: !l.envelope);
    } else if (ch == 'v' || (key == LogicalKeyboardKey.keyV && !shift)) {
      next = (l) => l.copyWith(volume: !l.volume);
    } else if (ch == 'm' || (key == LogicalKeyboardKey.keyM && !shift)) {
      next = (l) => l.copyWith(ma: !l.ma);
    } else if (ch == 'w' || (key == LogicalKeyboardKey.keyW && !shift)) {
      next = (l) => l.copyWith(wicks: !l.wicks);
    } else if (ch == 'g' || (key == LogicalKeyboardKey.keyG && !shift)) {
      next = (l) => l.copyWith(gaps: !l.gaps);
    } else if (ch == 'z' || (key == LogicalKeyboardKey.keyZ && !shift)) {
      next = (l) => l.copyWith(zeroVolume: !l.zeroVolume);
    } else if (ch == '#' ||
        key == LogicalKeyboardKey.numberSign ||
        (key == LogicalKeyboardKey.digit3 && shift)) {
      next = (l) => l.copyWith(grid: !l.grid);
    } else if (ch == 'x' || (key == LogicalKeyboardKey.keyX && !shift)) {
      next = (l) => l.copyWith(crosshair: !l.crosshair);
    } else if (ch == 'y' || (key == LogicalKeyboardKey.keyY && !shift)) {
      next = (l) => l.copyWith(priceAxis: !l.priceAxis);
    } else if (ch == 't' || (key == LogicalKeyboardKey.keyT && !shift)) {
      next = (l) => l.copyWith(timeAxis: !l.timeAxis);
    }
    if (next == null) return false;
    final cur = ref.read(chartLayersProvider);
    ref.read(chartLayersProvider.notifier).state = next(cur);
    return true;
  }

  bool _zoomOrPan(LogicalKeyboardKey key, String? ch, bool shift) {
    final plus =
        ch == '+' ||
        key == LogicalKeyboardKey.add ||
        key == LogicalKeyboardKey.numpadAdd ||
        (key == LogicalKeyboardKey.equal && shift);
    final minus =
        ch == '-' ||
        ch == '−' ||
        key == LogicalKeyboardKey.minus ||
        key == LogicalKeyboardKey.numpadSubtract;
    if (plus) {
      _zoom(1.15);
      return true;
    }
    if (minus) {
      _zoom(1 / 1.15);
      return true;
    }
    if (ch == 'h' || (key == LogicalKeyboardKey.keyH && !shift)) {
      _pan(-0.25);
      return true;
    }
    if (ch == 'l' || (key == LogicalKeyboardKey.keyL && !shift)) {
      _pan(0.25);
      return true;
    }
    if (key == LogicalKeyboardKey.pageUp) {
      _pan(-1);
      return true;
    }
    if (key == LogicalKeyboardKey.pageDown) {
      _pan(1);
      return true;
    }
    if (key == LogicalKeyboardKey.home) {
      _jump(latest: false);
      return true;
    }
    if (key == LogicalKeyboardKey.end ||
        ch == '0' ||
        key == LogicalKeyboardKey.digit0 ||
        key == LogicalKeyboardKey.numpad0) {
      _jump(latest: true);
      return true;
    }
    return false;
  }

  Future<void> _pickChartPeriod(
    BuildContext context, {
    required bool start,
    required bool typed,
  }) async {
    final bars = ref.read(barsProvider).value;
    if (bars == null || bars.isEmpty) return;
    final ts = typed
        ? await showUtcDatePrompt(
            context,
            title: start
                ? 'Period start (YYYY-MM-DD [HH:MM])'
                : 'Period end (YYYY-MM-DD [HH:MM])',
          )
        : await pickUtcMs(context, null);
    if (ts == null || !context.mounted) return;
    final i = nearestBarIndex(bars, ts);
    final v = ref.read(chartViewportProvider);
    ref.read(chartViewportProvider.notifier).state =
        start ? v.periodStart(i, bars.length) : v.periodEnd(i, bars.length);
  }

  Future<void> _jumpChartDate(BuildContext context) async {
    final bars = ref.read(barsProvider).value;
    if (bars == null || bars.isEmpty) return;
    final ts = await showUtcDatePrompt(
      context,
      title: 'Jump to date (YYYY-MM-DD [HH:MM])',
    );
    if (ts == null || !context.mounted) return;
    final i = nearestBarIndex(bars, ts);
    final v = ref.read(chartViewportProvider);
    ref.read(chartViewportProvider.notifier).state = v.jumpTo(i, bars.length);
  }

  void _zoom(double factor) {
    final n = ref.read(barsProvider).value?.length ?? 0;
    if (n == 0) return;
    final v = ref.read(chartViewportProvider);
    ref.read(chartViewportProvider.notifier).state = v.zoomAt(
      v.scale * factor,
      n: n,
    );
  }

  void _pan(double screens) {
    final n = ref.read(barsProvider).value?.length ?? 0;
    if (n == 0) return;
    final v = ref.read(chartViewportProvider);
    var delta = (v.visibleCount(n) * screens).round();
    if (delta == 0 && screens != 0) delta = screens < 0 ? -1 : 1;
    ref.read(chartViewportProvider.notifier).state = v.panBy(delta, n);
  }

  void _jump({required bool latest}) {
    final n = ref.read(barsProvider).value?.length ?? 0;
    if (n == 0) return;
    final v = ref.read(chartViewportProvider);
    ref.read(chartViewportProvider.notifier).state = ChartViewport(
      scale: v.scale,
      start: latest ? n : 0,
    ).clampTo(n);
  }

  void _selectAdjacentDataset(int delta) {
    final o = ref.read(overviewProvider).value;
    if (o == null || o.datasets.isEmpty) return;
    final range = ref.read(timeRangeProvider);
    final shown = o.datasets.where((d) => d.overlaps(range)).toList();
    if (shown.isEmpty) return;
    final key = ref.read(selectedDatasetProvider);
    var i = shown.indexWhere((d) => d.key == key);
    if (i < 0) {
      i = delta > 0 ? 0 : shown.length - 1;
    } else {
      i = (i + delta).clamp(0, shown.length - 1);
    }
    ref.read(selectedDatasetProvider.notifier).state = shown[i].key;
  }
}
