/// Data: the datasets (like the TUI sidebar, scoped by the time range) and the selected
/// one as a chart or a list. Desktop: list and chart side by side. Phone: list, then the
/// chart on its own page.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../shell.dart' show phoneBreakpoint;
import '../state.dart';
import '../theme/muninn_theme.dart';
import '../time_range.dart';
import 'chart.dart';
import 'unlock.dart';

class DataView extends ConsumerWidget {
  const DataView({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) => Unlocked(
        child: LayoutBuilder(builder: (context, c) {
          final phone = c.maxWidth < phoneBreakpoint;
          if (phone) return DatasetList(onOpen: () => Navigator.of(context).push(MaterialPageRoute(builder: (_) => const _SeriesPage())));
          return Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            SizedBox(width: 320, child: DatasetList(onOpen: () {})),
            VerticalDivider(width: 1, color: context.muninn.border),
            const Expanded(child: SeriesPanel()),
          ]);
        }),
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
    return ref.watch(overviewProvider).when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Text('$e', style: TextStyle(color: m.error)),
          data: (o) {
            final all = o?.datasets ?? const <Dataset>[];
            final shown = all.where((d) => d.overlaps(range)).toList();
            if (shown.isEmpty) {
              return Padding(
                padding: const EdgeInsets.all(16),
                child: Text(all.isEmpty ? 'No bars yet: `mnnn ingest path DIR`' : 'No dataset has bars in this time range', style: TextStyle(color: m.muted)),
              );
            }
            return ListView(children: [
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
                    onOpen();
                  },
                ),
            ]);
          },
        );
  }
}

/// The selected dataset as a chart or list, with the chart/list switch.
class SeriesPanel extends ConsumerWidget {
  const SeriesPanel({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final d = ref.watch(selectedDatasetValueProvider);
    if (d == null) return Center(child: Text('Pick a dataset', style: TextStyle(color: m.muted)));
    final asChart = ref.watch(dataAsChartProvider);
    final range = ref.watch(timeRangeProvider);
    return Padding(
      padding: const EdgeInsets.all(12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Wrap(spacing: 12, runSpacing: 8, crossAxisAlignment: WrapCrossAlignment.center, alignment: WrapAlignment.spaceBetween, children: [
          Text('${d.source} · ${d.symbol} · ${d.duration}', style: TextStyle(color: m.header, fontSize: 20, fontWeight: FontWeight.w800)),
          SegmentedButton<bool>(
            segments: const [
              ButtonSegment(value: true, icon: Icon(Icons.show_chart), label: Text('Chart')),
              ButtonSegment(value: false, icon: Icon(Icons.list), label: Text('List')),
            ],
            selected: {asChart},
            onSelectionChanged: (s) => ref.read(dataAsChartProvider.notifier).state = s.first,
          ),
        ]),
        Text(
          range.isAll ? '${fmtUtc(d.first)} → ${fmtUtc(d.last)} UTC' : 'Time range: ${range.from == null ? '…' : fmtUtc(range.from!)} → ${range.to == null ? '…' : fmtUtc(range.to!)} UTC',
          style: TextStyle(color: m.muted),
        ),
        const SizedBox(height: 8),
        Expanded(
          child: ref.watch(barsProvider).when(
                loading: () => const Center(child: CircularProgressIndicator()),
                error: (e, _) => Text('$e', style: TextStyle(color: m.error)),
                data: (bars) => bars == null ? const SizedBox.shrink() : (asChart ? BarChart(bars: bars) : BarTable(bars: bars)),
              ),
        ),
      ]),
    );
  }
}

class _SeriesPage extends StatelessWidget {
  const _SeriesPage();
  @override
  Widget build(BuildContext context) => Scaffold(
        appBar: AppBar(title: const Text('Data')),
        body: const SafeArea(child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [RangeBar(), Expanded(child: SeriesPanel())])),
      );
}
