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
import 'views/project_compute.dart';
import 'views/project_collab.dart';
import 'views/projects.dart';
import 'workspace.dart';
import 'tiling_extras.dart';

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
  ref.invalidate(tileBarsProvider);
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
  setWorkspace(ref, ref.read(workspaceProvider).changeView());
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
      if (down) return _windowCommand(key, ch, shift);
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

    if (down && ctrl && !alt && !meta && key == LogicalKeyboardKey.keyB) {
      if (_tiling) {
        setWorkspace(ref, ref.read(workspaceProvider).toggleSidebar());
      }
      return KeyEventResult.handled;
    }
    if (down && alt && !ctrl && !meta) {
      final n = _digitOf(key, ch);
      if (n != null && n >= 1 && n <= 9) {
        if (_tiling) {
          final ws = ref.read(workspaceProvider);
          if (ws.tile(n) != null) setWorkspace(ref, ws.focusTile(n));
        }
        return KeyEventResult.handled;
      }
    }
    if (ctrl || alt || meta) return KeyEventResult.ignored;

    if (key == LogicalKeyboardKey.tab) {
      final ws = ref.read(workspaceProvider);
      if (ws.sidebarFocused) {
        setWorkspace(ref, ws.unfocusSidebar());
        return KeyEventResult.handled;
      }
      if (_tiling) {
        setWorkspace(ref, shift ? ws.focusPrev() : ws.focusNext());
        return KeyEventResult.handled;
      }
    }
    if (key == LogicalKeyboardKey.escape &&
        ref.read(workspaceProvider).sidebarFocused) {
      setWorkspace(ref, ref.read(workspaceProvider).unfocusSidebar());
      return KeyEventResult.handled;
    }
    if ((ch == '/' || key == LogicalKeyboardKey.slash) && !shift) {
      if (_tiling) {
        if (ref.read(sectionProvider) != Section.data) {
          ref.read(sectionProvider.notifier).state = Section.data;
        }
        setWorkspace(ref, ref.read(workspaceProvider).focusSidebar());
        return KeyEventResult.handled;
      }
    }

    // Shift+letter menu shortcuts (T, X, P, K, G, L, W, A, Y) — same characters as the TUI.
    if (ch == 'Y' || (key == LogicalKeyboardKey.keyY && shift)) {
      toggleHistory(ref);
      return KeyEventResult.handled;
    }
    if (ch == 'A' || (key == LogicalKeyboardKey.keyA && shift)) {
      showAuthorsDialog(context, ref);
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.keyU || ch == 'u' || ch == 'U') {
      stepHistory(ref, back: !shift).catchError((e) {
        if (context.mounted) {
          ScaffoldMessenger.of(context)
              .showSnackBar(SnackBar(content: Text('$e')));
        }
      });
      return KeyEventResult.handled;
    }
    if (ch == 'T' || (key == LogicalKeyboardKey.keyT && shift)) {
      showTimeRangePicker(context, ref);
      return KeyEventResult.handled;
    }
    if (ch == 'X' || (key == LogicalKeyboardKey.keyX && shift)) {
      clearDataFilters(ref);
      return KeyEventResult.handled;
    }
    if (ch == 'P' || (key == LogicalKeyboardKey.keyP && shift)) {
      ref.read(sectionProvider.notifier).state = Section.projects;
      return KeyEventResult.handled;
    }
    if (ch == 'J' || (key == LogicalKeyboardKey.keyJ && shift)) {
      ref.read(projectPaneProvider.notifier).state = ProjectPane.journals;
      ref.read(sectionProvider.notifier).state = Section.projects;
      return KeyEventResult.handled;
    }
    if (ch == 'D' || (key == LogicalKeyboardKey.keyD && shift)) {
      ref.read(projectPaneProvider.notifier).state = ProjectPane.models;
      ref.read(sectionProvider.notifier).state = Section.projects;
      return KeyEventResult.handled;
    }
    if (ch == 'K' || (key == LogicalKeyboardKey.keyK && shift)) {
      ref.read(sectionProvider.notifier).state = Section.net;
      return KeyEventResult.handled;
    }
    if (ch == 'W' || (key == LogicalKeyboardKey.keyW && shift)) {
      ref.read(netPaneProvider.notifier).state = NetPane.collab;
      ref.read(sectionProvider.notifier).state = Section.net;
      return KeyEventResult.handled;
    }
    if (ch == 'G' || (key == LogicalKeyboardKey.keyG && shift)) {
      ref.read(sectionProvider.notifier).state = Section.orgs;
      return KeyEventResult.handled;
    }
    if (ch == 'C' || (key == LogicalKeyboardKey.keyC && shift)) {
      ref.read(projectPaneProvider.notifier).state = ProjectPane.code;
      ref.read(sectionProvider.notifier).state = Section.projects;
      return KeyEventResult.handled;
    }
    if (ch == 'E' || (key == LogicalKeyboardKey.keyE && shift)) {
      ref.read(projectPaneProvider.notifier).state = ProjectPane.pipelines;
      ref.read(sectionProvider.notifier).state = Section.projects;
      return KeyEventResult.handled;
    }
    if (ch == 'R' || (key == LogicalKeyboardKey.keyR && shift)) {
      ref.read(projectPaneProvider.notifier).state = ProjectPane.rules;
      ref.read(sectionProvider.notifier).state = Section.projects;
      return KeyEventResult.handled;
    }
    if (ch == 'V' || (key == LogicalKeyboardKey.keyV && shift)) {
      ref.read(projectPaneProvider.notifier).state = ProjectPane.reviews;
      ref.read(sectionProvider.notifier).state = Section.projects;
      return KeyEventResult.handled;
    }
    if (ch == '!') {
      ref.read(projectPaneProvider.notifier).state = ProjectPane.notifications;
      ref.read(sectionProvider.notifier).state = Section.projects;
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
      switch (ref.read(projectPaneProvider)) {
        case ProjectPane.code:
          showNewCodeSpaceDialog(context, ref);
        case ProjectPane.pipelines:
          showNewPipelineDialog(context, ref);
        case ProjectPane.rules:
          showNewRuleDialog(context, ref);
        case ProjectPane.reviews:
          showNewReviewDialog(context, ref);
        case ProjectPane.notifications:
          showSendNotificationDialog(context, ref);
        default:
          showNewProjectDialog(context, ref);
      }
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
          !ref.read(workspaceProvider).sidebarFocused &&
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

  bool get _tiling => MediaQuery.sizeOf(context).width >= 720;

  bool get _chartShowing =>
      ref.read(sectionProvider) == Section.data &&
      !ref.read(workspaceProvider).sidebarFocused &&
      ref.read(workspaceProvider).focused.asChart;

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
    final cur = ref.read(workspaceProvider).focused.layers;
    applyFocusedLayers(ref, next(cur));
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

  List<BarPoint>? _focusedBars() {
    final key = ref.read(workspaceProvider).focused.datasetKey;
    if (key != null) {
      final rows = ref.read(tileBarsProvider(key)).value;
      if (rows != null) return rows;
    }
    return ref.read(barsProvider).value;
  }

  Future<void> _pickChartPeriod(
    BuildContext context, {
    required bool start,
    required bool typed,
  }) async {
    final bars = _focusedBars();
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
    final v = ref.read(workspaceProvider).focused.viewport;
    applyFocusedViewport(
      ref,
      start ? v.periodStart(i, bars.length) : v.periodEnd(i, bars.length),
    );
  }

  Future<void> _jumpChartDate(BuildContext context) async {
    final bars = _focusedBars();
    if (bars == null || bars.isEmpty) return;
    final ts = await showUtcDatePrompt(
      context,
      title: 'Jump to date (YYYY-MM-DD [HH:MM])',
    );
    if (ts == null || !context.mounted) return;
    final i = nearestBarIndex(bars, ts);
    final v = ref.read(workspaceProvider).focused.viewport;
    applyFocusedViewport(ref, v.jumpTo(i, bars.length));
  }

  void _zoom(double factor) {
    final n = _focusedBars()?.length ?? 0;
    if (n == 0) return;
    final v = ref.read(workspaceProvider).focused.viewport;
    applyFocusedViewport(ref, v.zoomAt(v.scale * factor, n: n));
  }

  void _pan(double screens) {
    final n = _focusedBars()?.length ?? 0;
    if (n == 0) return;
    final v = ref.read(workspaceProvider).focused.viewport;
    var delta = (v.visibleCount(n) * screens).round();
    if (delta == 0 && screens != 0) delta = screens < 0 ? -1 : 1;
    applyFocusedViewport(ref, v.panBy(delta, n));
  }

  void _jump({required bool latest}) {
    final n = _focusedBars()?.length ?? 0;
    if (n == 0) return;
    final v = ref.read(workspaceProvider).focused.viewport;
    applyFocusedViewport(
      ref,
      ChartViewport(scale: v.scale, start: latest ? n : 0).clampTo(n),
    );
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
    setWorkspace(
      ref,
      ref.read(workspaceProvider).setFocusedDataset(shown[i].key),
    );
  }

  int? _digitOf(LogicalKeyboardKey key, String? ch) {
    final digits = <LogicalKeyboardKey, int>{
      LogicalKeyboardKey.digit1: 1,
      LogicalKeyboardKey.digit2: 2,
      LogicalKeyboardKey.digit3: 3,
      LogicalKeyboardKey.digit4: 4,
      LogicalKeyboardKey.digit5: 5,
      LogicalKeyboardKey.digit6: 6,
      LogicalKeyboardKey.digit7: 7,
      LogicalKeyboardKey.digit8: 8,
      LogicalKeyboardKey.digit9: 9,
    };
    if (digits[key] != null) return digits[key];
    if (ch != null &&
        ch.length == 1 &&
        ch.codeUnitAt(0) >= 49 &&
        ch.codeUnitAt(0) <= 57) {
      return ch.codeUnitAt(0) - 48;
    }
    return null;
  }

  KeyEventResult _windowCommand(
    LogicalKeyboardKey key,
    String? ch,
    bool shift,
  ) {
    final tiling = _tiling;
    final ws = ref.read(workspaceProvider);
    final area = ref.read(workspaceAreaProvider);

    void apply(Workspace next) => setWorkspace(ref, next);
    void applyOp(WsOp op) => applyWorkspace(ref, op, context: context);

    if ((ch == 't' || key == LogicalKeyboardKey.keyT) && !shift) {
      toggleDataView(ref);
      return KeyEventResult.handled;
    }
    if (!tiling) return KeyEventResult.handled;

    if (ch == 'v' || key == LogicalKeyboardKey.keyV) {
      applyOp(ws.split(SplitDir.horizontal, area));
      return KeyEventResult.handled;
    }
    if (ch == 's' || key == LogicalKeyboardKey.keyS) {
      applyOp(ws.split(SplitDir.vertical, area));
      return KeyEventResult.handled;
    }
    if (ch == 'q' ||
        ch == 'c' ||
        key == LogicalKeyboardKey.keyQ ||
        key == LogicalKeyboardKey.keyC) {
      applyOp(ws.closeTile());
      return KeyEventResult.handled;
    }
    if (ch == 'h' ||
        key == LogicalKeyboardKey.keyH ||
        key == LogicalKeyboardKey.arrowLeft) {
      apply(ws.focusDir(FocusDir.left, area) ?? ws);
      return KeyEventResult.handled;
    }
    if (ch == 'j' ||
        key == LogicalKeyboardKey.keyJ ||
        key == LogicalKeyboardKey.arrowDown) {
      apply(ws.focusDir(FocusDir.down, area) ?? ws);
      return KeyEventResult.handled;
    }
    if (ch == 'k' ||
        key == LogicalKeyboardKey.keyK ||
        key == LogicalKeyboardKey.arrowUp) {
      apply(ws.focusDir(FocusDir.up, area) ?? ws);
      return KeyEventResult.handled;
    }
    if (ch == 'l' ||
        key == LogicalKeyboardKey.keyL ||
        key == LogicalKeyboardKey.arrowRight) {
      apply(ws.focusDir(FocusDir.right, area) ?? ws);
      return KeyEventResult.handled;
    }
    if ((key == LogicalKeyboardKey.tab && !shift) ||
        ch == 'w' ||
        (key == LogicalKeyboardKey.keyW && !shift)) {
      apply(ws.focusNext());
      return KeyEventResult.handled;
    }
    if ((key == LogicalKeyboardKey.tab && shift) ||
        ch == 'W' ||
        (key == LogicalKeyboardKey.keyW && shift) ||
        ch == 'p' ||
        key == LogicalKeyboardKey.keyP) {
      apply(ws.focusPrev());
      return KeyEventResult.handled;
    }
    if (ch == '>' || (key == LogicalKeyboardKey.period && shift)) {
      applyOp(ws.resize(FocusDir.right, area));
      return KeyEventResult.handled;
    }
    if (ch == '<' || (key == LogicalKeyboardKey.comma && shift)) {
      applyOp(ws.resize(FocusDir.left, area));
      return KeyEventResult.handled;
    }
    if (ch == '+' ||
        key == LogicalKeyboardKey.add ||
        (key == LogicalKeyboardKey.equal && shift)) {
      applyOp(ws.resize(FocusDir.down, area));
      return KeyEventResult.handled;
    }
    if (ch == '-' || key == LogicalKeyboardKey.minus) {
      applyOp(ws.resize(FocusDir.up, area));
      return KeyEventResult.handled;
    }
    if (ch == '=' || key == LogicalKeyboardKey.equal) {
      apply(ws.equalize());
      return KeyEventResult.handled;
    }
    if (ch == 'x' || key == LogicalKeyboardKey.keyX) {
      applyOp(ws.swap());
      return KeyEventResult.handled;
    }
    if (ch == 'z' || key == LogicalKeyboardKey.keyZ) {
      apply(ws.toggleZoom());
      return KeyEventResult.handled;
    }
    if (ch == 'e' || key == LogicalKeyboardKey.keyE) {
      apply(ws.toggleNewDir());
      return KeyEventResult.handled;
    }
    if (ch == 'y' || key == LogicalKeyboardKey.keyY) {
      apply(ws.cycleLink());
      return KeyEventResult.handled;
    }
    return KeyEventResult.handled;
  }
}
