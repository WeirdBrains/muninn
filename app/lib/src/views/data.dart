/// Data: the datasets (like the TUI sidebar, scoped by the time range) and a
/// tiling workspace of chart/list tiles (the TUI content area). Desktop: sidebar
/// plus tiles. Phone: list, then the chart on its own page (no tiling).
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../shell.dart' show phoneBreakpoint;
import '../state.dart';
import '../theme/muninn_theme.dart';
import '../time_range.dart';
import '../workspace.dart';
import '../tiling_extras.dart';
import 'chart.dart';
import 'unlock.dart';

class DataView extends ConsumerWidget {
  const DataView({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) => Unlocked(
    child: LayoutBuilder(
      builder: (context, c) {
        final phone = c.maxWidth < phoneBreakpoint;
        if (phone) {
          return DatasetList(
            onOpen: () => Navigator.of(context)
                .push(MaterialPageRoute(builder: (_) => const _SeriesPage())),
          );
        }
        final ws = ref.watch(workspaceProvider);
        final m = context.muninn;
        final historyOpen = ref.watch(historyOpenProvider);
        return Row(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            if (historyOpen)
              const SizedBox(
                key: ValueKey('history-pane'),
                width: 280,
                child: HistoryPane(),
              ),
            if (ws.sidebarVisible) ...[
              SizedBox(
                width: 320,
                child: DecoratedBox(
                  decoration: BoxDecoration(
                    border: Border(
                      right: BorderSide(
                        color: ws.sidebarFocused ? m.borderFocus : m.border,
                        width: ws.sidebarFocused ? 2 : 1,
                      ),
                    ),
                  ),
                  child: DatasetList(onOpen: () {}),
                ),
              ),
              VerticalDivider(width: 1, color: m.border),
            ],
            const Expanded(child: WorkspacePane()),
          ],
        );
      },
    ),
  );
}

class DatasetList extends ConsumerWidget {
  const DatasetList({super.key, required this.onOpen});
  final VoidCallback onOpen;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final range = ref.watch(timeRangeProvider);
    final sel = ref.watch(selectedDatasetProvider);
    return ref
        .watch(overviewProvider)
        .when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Text('$e', style: TextStyle(color: m.error)),
          data: (o) {
            final all = o?.datasets ?? const <Dataset>[];
            final shown = all.where((d) => d.overlaps(range)).toList();
            if (shown.isEmpty) {
              return Padding(
                padding: const EdgeInsets.all(16),
                child: Text(
                  all.isEmpty
                      ? 'No bars yet: `mnnn ingest path DIR`'
                      : 'No dataset has bars in this time range',
                  style: TextStyle(color: m.muted),
                ),
              );
            }
            return ListView(
              children: [
                for (final d in shown)
                  ListTile(
                    minTileHeight: 56,
                    selected: d.key == sel,
                    selectedTileColor: m.selectionBg,
                    selectedColor: m.selectionFg,
                    leading: Icon(Icons.candlestick_chart, color: m.up),
                    title: Text('${d.symbol} · ${d.duration}'),
                    subtitle: Text('${d.source} · ${d.count} bars'),
                    onTap: () {
                      ref.read(selectedDatasetProvider.notifier).state = d.key;
                      setWorkspace(
                        ref,
                        ref
                            .read(workspaceProvider)
                            .setFocusedDataset(d.key)
                            .unfocusSidebar(),
                      );
                      onOpen();
                    },
                  ),
              ],
            );
          },
        );
  }
}

/// The tiling content area (TUI tiles). One tile until you split.
class WorkspacePane extends ConsumerWidget {
  const WorkspacePane({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final ws = ref.watch(workspaceProvider);
    return LayoutBuilder(
      builder: (context, c) {
        final area = Size(c.maxWidth, c.maxHeight);
        final prev = ref.read(workspaceAreaProvider);
        if (prev != area) {
          WidgetsBinding.instance.addPostFrameCallback((_) {
            if (ref.read(workspaceAreaProvider) != area) {
              ref.read(workspaceAreaProvider.notifier).state = area;
            }
          });
        }
        return _buildNode(context, ref, ws, ws.root, area);
      },
    );
  }
}

Widget _buildNode(
  BuildContext context,
  WidgetRef ref,
  Workspace ws,
  WsNode node,
  Size area,
) {
  if (ws.zoomed) {
    return _TileChrome(
      tile: ws.focused,
      child: SeriesPanel(tileId: ws.focused.id),
    );
  }
  switch (node) {
    case WsLeaf(:final tile):
      return _TileChrome(
        tile: tile,
        child: SeriesPanel(tileId: tile.id),
      );
    case WsSplit(
      :final dir,
      :final ratio,
      :final a,
      :final b,
      :final mode,
      :final active,
    ):
      if (mode != ContainerMode.split) {
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            _ContainerBar(split: node),
            Expanded(
              child: _buildNode(context, ref, ws, active == 0 ? a : b, area),
            ),
          ],
        );
      }
      final flexA = mathMax(1, (ratio * 1000).round());
      final flexB = mathMax(1, ((1 - ratio) * 1000).round());
      if (dir == SplitDir.horizontal) {
        return Row(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Expanded(flex: flexA, child: _buildNode(context, ref, ws, a, area)),
            VerticalDivider(width: 1, color: context.muninn.border),
            Expanded(flex: flexB, child: _buildNode(context, ref, ws, b, area)),
          ],
        );
      }
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Expanded(flex: flexA, child: _buildNode(context, ref, ws, a, area)),
          Divider(height: 1, color: context.muninn.border),
          Expanded(flex: flexB, child: _buildNode(context, ref, ws, b, area)),
        ],
      );
  }
}

int mathMax(int a, int b) => a > b ? a : b;

class _ContainerBar extends ConsumerWidget {
  const _ContainerBar({required this.split});
  final WsSplit split;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final titles = [split.a, split.b].map((n) {
      final t = n.tiles().first;
      final view = t.asChart ? 'chart' : 'list';
      return ('$view ${t.datasetKey ?? ''}'.trim(), t.id);
    }).toList();
    final stacked = split.mode == ContainerMode.stacked;
    return Material(
      color: m.bg,
      child: stacked
          ? Column(
              children: [
                for (var i = 0; i < titles.length; i++)
                  _tabChip(
                    context,
                    ref,
                    titles[i].$1,
                    titles[i].$2,
                    split.active == i,
                  ),
              ],
            )
          : Row(
              children: [
                for (var i = 0; i < titles.length; i++)
                  Expanded(
                    child: _tabChip(
                      context,
                      ref,
                      titles[i].$1,
                      titles[i].$2,
                      split.active == i,
                    ),
                  ),
              ],
            ),
    );
  }

  Widget _tabChip(
    BuildContext context,
    WidgetRef ref,
    String label,
    int id,
    bool on,
  ) {
    final m = context.muninn;
    return InkWell(
      onTap: () => setWorkspace(ref, ref.read(workspaceProvider).focusTile(id)),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
        color: on ? m.selectionBg : null,
        child: Text(
          label,
          overflow: TextOverflow.ellipsis,
          style: TextStyle(
            color: on ? m.selectionFg : m.muted,
            fontWeight: FontWeight.w600,
          ),
        ),
      ),
    );
  }
}

class _TileChrome extends ConsumerWidget {
  const _TileChrome({required this.tile, required this.child});
  final WsTile tile;
  final Widget child;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final ws = ref.watch(workspaceProvider);
    final focused = ws.focus == tile.id && !ws.sidebarFocused;
    final view = tile.asChart ? 'chart' : 'list';
    final link = tile.link == null ? '' : ' ⛓${tile.link}';
    return GestureDetector(
      onTap: () => setWorkspace(ref, ws.focusTile(tile.id)),
      child: Container(
        key: ValueKey('data-tile-${tile.id}'),
        decoration: BoxDecoration(
          border: Border.all(
            color: focused ? m.borderFocus : m.border,
            width: focused ? 2 : 1,
          ),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(8, 4, 8, 0),
              child: Text(
                '[${tile.id}] $view$link',
                style: TextStyle(
                  color: focused ? m.header : m.muted,
                  fontWeight: FontWeight.w700,
                  fontSize: 12,
                ),
              ),
            ),
            Expanded(child: child),
          ],
        ),
      ),
    );
  }
}

/// The selected dataset as a chart or list, with the chart/list switch.
class SeriesPanel extends ConsumerWidget {
  const SeriesPanel({super.key, this.tileId});
  final int? tileId;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final ws = ref.watch(workspaceProvider);
    final tile = tileId == null ? null : ws.tile(tileId!);
    final key = tile?.datasetKey ?? ref.watch(selectedDatasetProvider);
    final bool asChart = tile?.asChart ?? ref.watch(dataAsChartProvider);
    final o = ref.watch(overviewProvider).value;
    Dataset? d;
    if (key != null && o != null) {
      for (final x in o.datasets) {
        if (x.key == key) d = x;
      }
    }
    d ??= ref.watch(selectedDatasetValueProvider);
    if (d == null) {
      return Center(
        child: Text('Pick a dataset', style: TextStyle(color: m.muted)),
      );
    }
    final range = ref.watch(timeRangeProvider);
    final bars = tile?.datasetKey != null
        ? ref.watch(tileBarsProvider(tile!.datasetKey!))
        : ref.watch(barsProvider);
    return Padding(
      padding: const EdgeInsets.all(12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Wrap(
            spacing: 12,
            runSpacing: 8,
            crossAxisAlignment: WrapCrossAlignment.center,
            alignment: WrapAlignment.spaceBetween,
            children: [
              Text(
                '${d.source} · ${d.symbol} · ${d.duration}',
                style: TextStyle(
                  color: m.header,
                  fontSize: 20,
                  fontWeight: FontWeight.w800,
                ),
              ),
              Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  IconButton(
                    tooltip: 'Chart layers',
                    icon: const Icon(Icons.layers_outlined),
                    iconSize: 28,
                    onPressed: () =>
                        showChartLayersSheet(context, ref, tileId: tile?.id),
                  ),
                  SegmentedButton<bool>(
                    segments: const [
                      ButtonSegment(
                        value: true,
                        icon: Icon(Icons.show_chart),
                        label: Text('Chart'),
                      ),
                      ButtonSegment(
                        value: false,
                        icon: Icon(Icons.list),
                        label: Text('List'),
                      ),
                    ],
                    selected: {asChart},
                    onSelectionChanged: (s) {
                      final next = s.first;
                      final id = tile?.id ?? ref.read(workspaceProvider).focus;
                      setWorkspace(
                        ref,
                        ref.read(workspaceProvider).setTileView(id, next),
                      );
                    },
                  ),
                ],
              ),
            ],
          ),
          Text(
            range.isAll
                ? '${fmtUtc(d.first)} → ${fmtUtc(d.last)} UTC'
                : 'Time range: ${range.from == null ? '…' : fmtUtc(range.from!)} → ${range.to == null ? '…' : fmtUtc(range.to!)} UTC',
            style: TextStyle(color: m.muted),
          ),
          const SizedBox(height: 8),
          Expanded(
            child: bars.when(
              loading: () => const Center(child: CircularProgressIndicator()),
              error: (e, _) => Text('$e', style: TextStyle(color: m.error)),
              data: (rows) => rows == null
                  ? const SizedBox.shrink()
                  : (asChart
                        ? BarChart(
                            bars: rows,
                            layers:
                                tile?.layers ?? ref.watch(chartLayersProvider),
                            tileId: tile?.id,
                          )
                        : BarTable(
                            bars: rows,
                            selectedTs: tile?.listCursorTs,
                            onSelect: tile == null
                                ? null
                                : (ts) => syncLinkedFromList(ref, tile.id, ts),
                          )),
            ),
          ),
        ],
      ),
    );
  }
}

class _SeriesPage extends StatelessWidget {
  const _SeriesPage();
  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(title: const Text('Data')),
    body: const SafeArea(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          RangeBar(),
          Expanded(child: SeriesPanel()),
        ],
      ),
    ),
  );
}
