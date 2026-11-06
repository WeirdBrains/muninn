/// Desktop tiling workspace, matching the TUI's i3-style binary tree
/// (`docs/tiling.md`): split, close, focus, resize, zoom, link, containers,
/// presets. The live tree is session-only (`[tui.layout]` is not written on
/// start); named layouts save to `[tui.layouts]` and are shared with the TUI.
library;

import 'dart:convert';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/legacy.dart';

import 'api.dart';
import 'state.dart';

/// Pixel stand-in for the TUI's 20×6 cell minimum.
const double kMinTileW = 200;
const double kMinTileH = 96;
const double kResizeStep = 0.05;

enum SplitDir { horizontal, vertical }

enum ContainerMode { split, tabbed, stacked }

enum LayoutPreset { twoUp, threeColumn, mainStack, grid2x2 }

enum FocusDir { left, right, up, down }

extension on SplitDir {
  SplitDir get flip =>
      this == SplitDir.horizontal ? SplitDir.vertical : SplitDir.horizontal;
}

class WsTile {
  const WsTile({
    required this.id,
    this.asChart = true,
    this.datasetKey,
    this.link,
    this.layers = const ChartLayers(),
    this.viewport = const ChartViewport(),
    this.listCursorTs,
    this.frameKey,
  });
  final int id;
  final bool asChart;
  final String? datasetKey;
  final int? link;
  final ChartLayers layers;
  final ChartViewport viewport;
  final int? listCursorTs;
  /// `monitor:PIPELINE`, shared with the TUI's `FrameTile.key`.
  final String? frameKey;

  bool get isMonitor =>
      frameKey != null && frameKey!.startsWith('monitor:') && frameKey!.length > 8;

  String? get monitorPipeline =>
      isMonitor ? frameKey!.substring('monitor:'.length) : null;

  String get label {
    if (isMonitor) return 'monitor · $monitorPipeline';
    final view = asChart ? 'chart' : 'list';
    return '$view ${datasetKey ?? ''}'.trim();
  }

  WsTile copyWith({
    bool? asChart,
    String? datasetKey,
    int? link,
    bool clearLink = false,
    bool clearDataset = false,
    ChartLayers? layers,
    ChartViewport? viewport,
    int? listCursorTs,
    bool clearListCursor = false,
    String? frameKey,
    bool clearFrame = false,
  }) => WsTile(
    id: id,
    asChart: asChart ?? this.asChart,
    datasetKey: clearDataset ? datasetKey : (datasetKey ?? this.datasetKey),
    link: clearLink ? link : (link ?? this.link),
    layers: layers ?? this.layers,
    viewport: viewport ?? this.viewport,
    listCursorTs: clearListCursor
        ? listCursorTs
        : (listCursorTs ?? this.listCursorTs),
    frameKey: clearFrame ? frameKey : (frameKey ?? this.frameKey),
  );
}

sealed class WsNode {
  const WsNode();
  List<WsTile> tiles();
  WsNode mapTiles(WsTile Function(WsTile) f);
}

class WsLeaf extends WsNode {
  const WsLeaf(this.tile);
  final WsTile tile;
  @override
  List<WsTile> tiles() => [tile];
  @override
  WsNode mapTiles(WsTile Function(WsTile) f) => WsLeaf(f(tile));
}

class WsSplit extends WsNode {
  const WsSplit({
    required this.dir,
    required this.ratio,
    required this.a,
    required this.b,
    this.mode = ContainerMode.split,
    this.active = 0,
  });
  final SplitDir dir;
  final double ratio;
  final WsNode a, b;
  final ContainerMode mode;
  final int active;
  @override
  List<WsTile> tiles() => [...a.tiles(), ...b.tiles()];
  @override
  WsNode mapTiles(WsTile Function(WsTile) f) => WsSplit(
    dir: dir,
    ratio: ratio,
    a: a.mapTiles(f),
    b: b.mapTiles(f),
    mode: mode,
    active: active,
  );
}

class WsOp {
  const WsOp(this.workspace, {this.error, this.status});
  final Workspace workspace;
  final String? error;
  final String? status;
  bool get ok => error == null;
}

class Workspace {
  const Workspace({
    required this.root,
    required this.focus,
    required this.nextId,
    this.zoomed = false,
    this.newDir = SplitDir.horizontal,
    this.sidebarVisible = true,
    this.sidebarFocused = false,
  });

  final WsNode root;
  final int focus;
  final int nextId;
  final bool zoomed;
  final SplitDir newDir;
  final bool sidebarVisible;
  final bool sidebarFocused;

  factory Workspace.single({String? datasetKey, bool asChart = true}) =>
      Workspace(
        root: WsLeaf(WsTile(id: 1, asChart: asChart, datasetKey: datasetKey)),
        focus: 1,
        nextId: 2,
      );

  List<WsTile> get tiles => root.tiles();

  WsTile get focused {
    for (final t in tiles) {
      if (t.id == focus) return t;
    }
    return tiles.first;
  }

  WsTile? tile(int id) {
    for (final t in tiles) {
      if (t.id == id) return t;
    }
    return null;
  }

  /// Tiles paired with [id], matching the TUI's `linked(id)` (either direction).
  List<WsTile> linkedTo(int id) {
    final me = tile(id)?.link;
    return [
      for (final t in tiles)
        if (t.id != id && (t.link == id || me == t.id)) t,
    ];
  }

  Workspace updateTile(int id, WsTile Function(WsTile) f) =>
      _copy(root: root.mapTiles((t) => t.id == id ? f(t) : t));

  /// Open or focus a pipeline monitor tile (`monitor:PIPELINE`), like the TUI's `m`.
  Workspace openMonitor(String pipeline, Size area) {
    final key = 'monitor:$pipeline';
    for (final t in tiles) {
      if (t.frameKey == key) return focusTile(t.id);
    }
    var ws = this;
    final blank = focused.datasetKey == null && focused.frameKey == null;
    if (!blank) {
      var op = split(SplitDir.horizontal, area);
      if (op.error != null) op = split(SplitDir.vertical, area);
      if (op.error != null) return this;
      ws = op.workspace;
    }
    return ws.updateTile(
      ws.focus,
      (t) => t.copyWith(frameKey: key, clearDataset: true, datasetKey: null),
    );
  }

  String encode() => jsonEncode(toJson());

  Map<String, dynamic> toJson() => {
    'focus': focus,
    'nextId': nextId,
    'zoomed': zoomed,
    'newDir': newDir == SplitDir.vertical ? 'vertical' : 'horizontal',
    'root': _nodeToJson(root),
  };

  static Workspace decode(String json) =>
      Workspace.fromJson(jsonDecode(json) as Map<String, dynamic>);

  factory Workspace.fromJson(Map<String, dynamic> j) => Workspace(
    root: _nodeFromJson(j['root']),
    focus: _asInt(j['focus'], 1),
    nextId: _asInt(j['nextId'], 2),
    zoomed: j['zoomed'] == true,
    newDir: j['newDir'] == 'vertical' ? SplitDir.vertical : SplitDir.horizontal,
  );

  Workspace _copy({
    WsNode? root,
    int? focus,
    int? nextId,
    bool? zoomed,
    SplitDir? newDir,
    bool? sidebarVisible,
    bool? sidebarFocused,
  }) => Workspace(
    root: root ?? this.root,
    focus: focus ?? this.focus,
    nextId: nextId ?? this.nextId,
    zoomed: zoomed ?? this.zoomed,
    newDir: newDir ?? this.newDir,
    sidebarVisible: sidebarVisible ?? this.sidebarVisible,
    sidebarFocused: sidebarFocused ?? this.sidebarFocused,
  );

  List<bool>? _pathTo(int id, [WsNode? node]) {
    final n = node ?? root;
    switch (n) {
      case WsLeaf(:final tile):
        return tile.id == id ? <bool>[] : null;
      case WsSplit(:final a, :final b):
        final pa = _pathTo(id, a);
        if (pa != null) return [false, ...pa];
        final pb = _pathTo(id, b);
        if (pb != null) return [true, ...pb];
        return null;
    }
  }

  WsNode _at(List<bool> path, [WsNode? node]) {
    var n = node ?? root;
    for (final step in path) {
      if (n is WsSplit) {
        n = step ? n.b : n.a;
      } else {
        break;
      }
    }
    return n;
  }

  WsNode _replace(List<bool> path, WsNode replacement, [WsNode? node]) {
    final n = node ?? root;
    if (path.isEmpty) return replacement;
    if (n is! WsSplit) return replacement;
    final head = path.first;
    final tail = path.sublist(1);
    return WsSplit(
      dir: n.dir,
      ratio: n.ratio,
      mode: n.mode,
      active: n.active,
      a: head ? n.a : _replace(tail, replacement, n.a),
      b: head ? _replace(tail, replacement, n.b) : n.b,
    );
  }

  Workspace _syncActive() {
    final path = _pathTo(focus);
    if (path == null) return this;
    var node = root;
    // Rebuild with active flags along the focus path.
    WsNode mark(WsNode n, List<bool> p) {
      if (n is! WsSplit || p.isEmpty) return n;
      final child = p.first;
      return WsSplit(
        dir: n.dir,
        ratio: n.ratio,
        mode: n.mode,
        active: child ? 1 : 0,
        a: child ? n.a : mark(n.a, p.sublist(1)),
        b: child ? mark(n.b, p.sublist(1)) : n.b,
      );
    }

    var next = _copy(root: mark(node, path));
    if (path.isNotEmpty) {
      final parent = next._at(path.sublist(0, path.length - 1));
      if (parent is WsSplit && parent.mode == ContainerMode.split) {
        next = next._copy(newDir: parent.dir);
      }
    }
    return next;
  }

  Workspace focusTile(int id) {
    if (tile(id) == null) return this;
    return _copy(focus: id, sidebarFocused: false)._syncActive();
  }

  Workspace focusNext() {
    final ids = tiles.map((t) => t.id).toList();
    final i = ids.indexOf(focus);
    return focusTile(ids[(i + 1) % ids.length]);
  }

  Workspace focusPrev() {
    final ids = tiles.map((t) => t.id).toList();
    final i = ids.indexOf(focus);
    return focusTile(ids[(i + ids.length - 1) % ids.length]);
  }

  Workspace changeView() {
    final id = focused.id;
    return _copy(
      root: root.mapTiles(
        (t) => t.id == id ? t.copyWith(asChart: !t.asChart) : t,
      ),
    );
  }

  Workspace setTileView(int id, bool asChart) => _copy(
    root: root.mapTiles((t) => t.id == id ? t.copyWith(asChart: asChart) : t),
  );

  Workspace setFocusedDataset(String? key) {
    final id = focused.id;
    return _copy(
      root: root.mapTiles((t) => t.id == id ? t.copyWith(datasetKey: key) : t),
    );
  }

  Workspace toggleSidebar() => _copy(
    sidebarVisible: !sidebarVisible,
    sidebarFocused: sidebarVisible ? false : sidebarFocused,
  );

  Workspace focusSidebar() => _copy(sidebarVisible: true, sidebarFocused: true);

  Workspace unfocusSidebar() => _copy(sidebarFocused: false);

  Workspace toggleZoom() => _copy(zoomed: !zoomed);

  Workspace toggleNewDir() => _copy(newDir: newDir.flip);

  WsOp split(SplitDir dir, Size area, {bool? asChart}) {
    final r = rectOf(focus, area);
    if (r == null) return WsOp(this, error: 'focused tile is not visible');
    final ok = dir == SplitDir.horizontal
        ? r.width / 2 >= kMinTileW
        : r.height / 2 >= kMinTileH;
    if (!ok) {
      return WsOp(
        this,
        error:
            'tile too small to split (minimum ${kMinTileW.round()}×${kMinTileH.round()})',
      );
    }
    final path = _pathTo(focus);
    if (path == null) return WsOp(this, error: 'no focus');
    final old = _at(path);
    if (old is! WsLeaf) return WsOp(this, error: 'no focus');
    final id = nextId;
    final copied = WsTile(
      id: id,
      asChart: asChart ?? old.tile.asChart,
      datasetKey: old.tile.datasetKey,
      layers: old.tile.layers,
      viewport: old.tile.viewport,
    );
    final next = _replace(
      path,
      WsSplit(dir: dir, ratio: 0.5, a: old, b: WsLeaf(copied)),
    );
    return WsOp(
      _copy(
        root: next,
        focus: id,
        nextId: id + 1,
        zoomed: false,
        newDir: dir,
      ).unfocusSidebar()._syncActive(),
    );
  }

  WsOp closeTile() {
    final path = _pathTo(focus);
    if (path == null) return WsOp(this, error: 'no focus');
    if (path.isEmpty) return WsOp(this, error: "can't close the last tile");
    final parentPath = path.sublist(0, path.length - 1);
    final inB = path.last;
    final parent = _at(parentPath);
    if (parent is! WsSplit) {
      return WsOp(this, error: "can't close the last tile");
    }
    final sibling = inB ? parent.a : parent.b;
    final closed = focus;
    final nextFocus = sibling.tiles().first.id;
    final replaced = _replace(parentPath, sibling);
    final cleaned = replaced.mapTiles(
      (t) => t.link == closed ? t.copyWith(clearLink: true, link: null) : t,
    );
    return WsOp(
      _copy(
        root: cleaned,
        focus: nextFocus,
        zoomed: false,
      ).unfocusSidebar()._syncActive(),
    );
  }

  Workspace equalize() {
    WsNode eq(WsNode n) {
      if (n is! WsSplit) return n;
      final ua = _units(n.a, n.dir);
      final ub = _units(n.b, n.dir);
      return WsSplit(
        dir: n.dir,
        ratio: ua / (ua + ub),
        mode: n.mode,
        active: n.active,
        a: eq(n.a),
        b: eq(n.b),
      );
    }

    return _copy(root: eq(root));
  }

  double _units(WsNode n, SplitDir dir) {
    if (n is WsSplit && n.dir == dir && n.mode == ContainerMode.split) {
      return _units(n.a, dir) + _units(n.b, dir);
    }
    return 1;
  }

  WsOp resize(FocusDir d, Size area) {
    final want = (d == FocusDir.left || d == FocusDir.right)
        ? SplitDir.horizontal
        : SplitDir.vertical;
    final grow = d == FocusDir.right || d == FocusDir.down;
    final path = _pathTo(focus);
    if (path == null) return WsOp(this, error: 'no focus');
    for (var i = path.length - 1; i >= 0; i--) {
      final parentPath = path.sublist(0, i);
      final parent = _at(parentPath);
      if (parent is! WsSplit) continue;
      if (parent.dir != want || parent.mode != ContainerMode.split) continue;
      final inA = !path[i];
      final delta = (grow == inA) ? kResizeStep : -kResizeStep;
      final ratio = (parent.ratio + delta).clamp(0.05, 0.95);
      final updated = _replace(
        parentPath,
        WsSplit(
          dir: parent.dir,
          ratio: ratio,
          mode: parent.mode,
          active: parent.active,
          a: parent.a,
          b: parent.b,
        ),
      );
      final next = _copy(root: updated);
      if (!next.fits(area)) {
        return WsOp(
          this,
          error:
              'minimum tile size is ${kMinTileW.round()}×${kMinTileH.round()}',
        );
      }
      return WsOp(next);
    }
    return WsOp(this, error: 'no split in that direction');
  }

  bool fits(Size area) => (zoomed ? _copy(zoomed: false) : this)
      .rects(area)
      .every((p) => p.rect.width >= kMinTileW && p.rect.height >= kMinTileH);

  List<({int id, Rect rect})> rects(Size area) {
    final out = <({int id, Rect rect})>[];
    void place(WsNode n, Rect r) {
      switch (n) {
        case WsLeaf(:final tile):
          out.add((id: tile.id, rect: r));
        case WsSplit(
          :final dir,
          :final ratio,
          :final a,
          :final b,
          :final mode,
          :final active,
        ):
          if (mode == ContainerMode.split) {
            final (ra, rb) = _splitRect(r, dir, ratio);
            place(a, ra);
            place(b, rb);
          } else {
            final barH = math.min(
              mode == ContainerMode.tabbed ? 36.0 : 56.0,
              r.height,
            );
            final body = Rect.fromLTWH(
              r.left,
              r.top + barH,
              r.width,
              r.height - barH,
            );
            place(active == 0 ? a : b, body);
          }
      }
    }

    if (zoomed) {
      place(WsLeaf(focused), Rect.fromLTWH(0, 0, area.width, area.height));
    } else {
      place(root, Rect.fromLTWH(0, 0, area.width, area.height));
    }
    return out;
  }

  Rect? rectOf(int id, Size area) {
    for (final p in rects(area)) {
      if (p.id == id) return p.rect;
    }
    return null;
  }

  (Rect, Rect) _splitRect(Rect r, SplitDir dir, double ratio) {
    if (dir == SplitDir.horizontal) {
      final w = (r.width * ratio)
          .roundToDouble()
          .clamp(0.0, r.width)
          .toDouble();
      return (
        Rect.fromLTWH(r.left, r.top, w, r.height),
        Rect.fromLTWH(r.left + w, r.top, r.width - w, r.height),
      );
    }
    final h = (r.height * ratio)
        .roundToDouble()
        .clamp(0.0, r.height)
        .toDouble();
    return (
      Rect.fromLTWH(r.left, r.top, r.width, h),
      Rect.fromLTWH(r.left, r.top + h, r.width, r.height - h),
    );
  }

  Workspace? focusDir(FocusDir d, Size area) {
    final placed = rects(area);
    Rect? cur;
    for (final p in placed) {
      if (p.id == focus) cur = p.rect;
    }
    if (cur == null) return null;
    ({int id, Rect rect})? best;
    var bestDist = 1 << 30;
    for (final p in placed) {
      if (p.id == focus) continue;
      final r = p.rect;
      final ok = switch (d) {
        FocusDir.left =>
          r.right <= cur.left &&
              _overlaps(r.top, r.bottom, cur.top, cur.bottom),
        FocusDir.right =>
          r.left >= cur.right &&
              _overlaps(r.top, r.bottom, cur.top, cur.bottom),
        FocusDir.up =>
          r.bottom <= cur.top &&
              _overlaps(r.left, r.right, cur.left, cur.right),
        FocusDir.down =>
          r.top >= cur.bottom &&
              _overlaps(r.left, r.right, cur.left, cur.right),
      };
      if (!ok) continue;
      final cx = cur.left + cur.width / 2;
      final cy = cur.top + cur.height / 2;
      final x = r.left + r.width / 2;
      final y = r.top + r.height / 2;
      final dist = (x - cx).abs() + (y - cy).abs();
      if (dist < bestDist) {
        bestDist = dist.round();
        best = p;
      }
    }
    return best == null ? null : focusTile(best.id);
  }

  bool _overlaps(double a0, double a1, double b0, double b1) =>
      a0 < b1 && b0 < a1;

  WsOp swap() {
    final ids = tiles.map((t) => t.id).toList();
    if (ids.length < 2) return WsOp(this, error: 'nothing to swap with');
    final i = ids.indexOf(focus);
    final other = ids[(i + 1) % ids.length];
    final pa = _pathTo(focus);
    final pb = _pathTo(other);
    if (pa == null || pb == null) {
      return WsOp(this, error: 'nothing to swap with');
    }
    final ta = _at(pa);
    final tb = _at(pb);
    var next = _replace(pa, tb);
    // After first replace, paths to `other` are unchanged if disjoint.
    next = Workspace(
      root: next,
      focus: focus,
      nextId: nextId,
      zoomed: zoomed,
      newDir: newDir,
      sidebarVisible: sidebarVisible,
      sidebarFocused: sidebarFocused,
    )._replace(pb, ta);
    return WsOp(_copy(root: next).focusTile(focus));
  }

  Workspace cycleLink() {
    final ids = [
      for (final t in tiles)
        if (t.id != focus) t.id,
    ];
    final id = focused.id;
    final cur = focused.link;
    final int? next;
    if (ids.isEmpty) {
      next = null;
    } else if (cur == null) {
      next = ids.first;
    } else {
      final p = ids.indexOf(cur);
      next = (p >= 0 && p + 1 < ids.length) ? ids[p + 1] : null;
    }
    return _copy(
      root: root.mapTiles(
        (t) => t.id == id ? t.copyWith(clearLink: true, link: next) : t,
      ),
    );
  }

  WsOp setContainer(ContainerMode mode) {
    final path = _pathTo(focus);
    if (path == null || path.isEmpty) {
      return WsOp(
        this,
        error: 'a single tile has no container; split it first',
      );
    }
    final pp = path.sublist(0, path.length - 1);
    final parent = _at(pp);
    if (parent is! WsSplit) {
      return WsOp(
        this,
        error: 'a single tile has no container; split it first',
      );
    }
    return WsOp(
      _copy(
        root: _replace(
          pp,
          WsSplit(
            dir: parent.dir,
            ratio: parent.ratio,
            mode: mode,
            active: parent.active,
            a: parent.a,
            b: parent.b,
          ),
        ),
      )._syncActive(),
    );
  }

  Workspace preset(LayoutPreset p, {String? datasetKey, bool? asChart}) {
    final series = datasetKey ?? focused.datasetKey;
    final chart = asChart ?? focused.asChart;
    var id = 0;
    WsLeaf leaf({required bool asChart}) {
      id += 1;
      return WsLeaf(WsTile(id: id, asChart: asChart, datasetKey: series));
    }

    final WsNode next;
    switch (p) {
      case LayoutPreset.twoUp:
        next = WsSplit(
          dir: SplitDir.horizontal,
          ratio: 0.5,
          a: leaf(asChart: false),
          b: leaf(asChart: true),
        );
      case LayoutPreset.threeColumn:
        next = WsSplit(
          dir: SplitDir.horizontal,
          ratio: 1 / 3,
          a: leaf(asChart: false),
          b: WsSplit(
            dir: SplitDir.horizontal,
            ratio: 0.5,
            a: leaf(asChart: true),
            b: leaf(asChart: chart),
          ),
        );
      case LayoutPreset.mainStack:
        next = WsSplit(
          dir: SplitDir.horizontal,
          ratio: 0.6,
          a: leaf(asChart: true),
          b: WsSplit(
            dir: SplitDir.vertical,
            ratio: 0.5,
            a: leaf(asChart: false),
            b: leaf(asChart: true),
          ),
        );
      case LayoutPreset.grid2x2:
        next = WsSplit(
          dir: SplitDir.vertical,
          ratio: 0.5,
          a: WsSplit(
            dir: SplitDir.horizontal,
            ratio: 0.5,
            a: leaf(asChart: false),
            b: leaf(asChart: true),
          ),
          b: WsSplit(
            dir: SplitDir.horizontal,
            ratio: 0.5,
            a: leaf(asChart: true),
            b: leaf(asChart: false),
          ),
        );
    }
    return Workspace(
      root: next,
      focus: 1,
      nextId: id + 1,
      sidebarVisible: sidebarVisible,
    );
  }
}

String? _frameKey(dynamic v) {
  if (v is Map) {
    final key = v['key'];
    if (key is String && key.trim().isNotEmpty) return key;
  }
  return null;
}

int _asInt(dynamic v, [int d = 0]) {
  if (v is int) return v;
  if (v is num) return v.toInt();
  return d;
}

double _asDouble(dynamic v, [double d = 0]) {
  if (v is num) return v.toDouble();
  return d;
}

Map<String, dynamic> _nodeToJson(WsNode n) {
  switch (n) {
    case WsLeaf(:final tile):
      return {
        'type': 'tile',
        'id': tile.id,
        'asChart': tile.asChart,
        if (tile.datasetKey != null) 'datasetKey': tile.datasetKey,
        if (tile.link != null) 'link': tile.link,
        if (tile.frameKey != null)
          'frame': {'key': tile.frameKey, 'table': false, 'columns': <String>[]},
        'layers': tile.layers.toJson(),
        'viewportScale': tile.viewport.scale,
        'viewportStart': tile.viewport.start,
        if (tile.listCursorTs != null) 'listCursorTs': tile.listCursorTs,
      };
    case WsSplit(
      :final dir,
      :final ratio,
      :final a,
      :final b,
      :final mode,
      :final active,
    ):
      return {
        'type': 'split',
        'dir': dir == SplitDir.vertical ? 'vertical' : 'horizontal',
        'mode': mode.name,
        'ratio': ratio,
        'active': active,
        'a': _nodeToJson(a),
        'b': _nodeToJson(b),
      };
  }
}

WsNode _nodeFromJson(dynamic v) {
  final j = v is Map<String, dynamic> ? v : Map<String, dynamic>.from(v as Map);
  if (j['type'] == 'split') {
    return WsSplit(
      dir: j['dir'] == 'vertical' ? SplitDir.vertical : SplitDir.horizontal,
      ratio: _asDouble(j['ratio'], 0.5),
      mode: switch (j['mode']) {
        'tabbed' => ContainerMode.tabbed,
        'stacked' => ContainerMode.stacked,
        _ => ContainerMode.split,
      },
      active: _asInt(j['active']),
      a: _nodeFromJson(j['a']),
      b: _nodeFromJson(j['b']),
    );
  }
  final layers = j['layers'];
  return WsLeaf(
    WsTile(
      id: _asInt(j['id'], 1),
      asChart: j['asChart'] != false,
      datasetKey: j['datasetKey'] as String?,
      link: j['link'] == null ? null : _asInt(j['link']),
      frameKey: _frameKey(j['frame']),
      layers: ChartLayers.fromJson(
        layers is Map<String, dynamic>
            ? layers
            : (layers is Map ? Map<String, dynamic>.from(layers) : null),
      ),
      viewport: ChartViewport.fromParts(
        scale: j['viewportScale'] is num
            ? (j['viewportScale'] as num).toDouble()
            : null,
        start: j['viewportStart'] == null ? null : _asInt(j['viewportStart']),
      ),
      listCursorTs: j['listCursorTs'] == null
          ? null
          : _asInt(j['listCursorTs']),
    ),
  );
}

final workspaceProvider = StateProvider<Workspace>((ref) => Workspace.single());

final workspaceAreaProvider = StateProvider<Size>(
  (ref) => const Size(800, 600),
);

/// Bars for a tile's dataset (same query as [barsProvider], keyed by dataset).
final tileBarsProvider = FutureProvider.family<List<BarPoint>?, String>((
  ref,
  key,
) {
  final o = ref.watch(overviewProvider).value;
  final r = ref.watch(timeRangeProvider);
  if (o == null) return Future.value(null);
  for (final d in o.datasets) {
    if (d.key == key) return ref.watch(apiProvider).queryBars(d, r);
  }
  return Future.value(null);
});

void syncWorkspaceSelection(WidgetRef ref) {
  final ws = ref.read(workspaceProvider);
  final key = ws.focused.datasetKey;
  if (key != null) {
    ref.read(selectedDatasetProvider.notifier).state = key;
  }
  ref.read(dataAsChartProvider.notifier).state = ws.focused.asChart;
  ref.read(chartLayersProvider.notifier).state = ws.focused.layers;
  ref.read(chartViewportProvider.notifier).state = ws.focused.viewport;
}

void applyWorkspace(WidgetRef ref, WsOp op, {BuildContext? context}) {
  ref.read(workspaceProvider.notifier).state = op.workspace;
  syncWorkspaceSelection(ref);
  final msg = op.error ?? op.status;
  if (msg != null && context != null && context.mounted) {
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(msg)));
  }
}

void setWorkspace(WidgetRef ref, Workspace ws) {
  ref.read(workspaceProvider.notifier).state = ws;
  syncWorkspaceSelection(ref);
}
