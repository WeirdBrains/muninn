/// Data ▸ Filters, Sort and Coverage & gaps. The time range stays on its own picker;
/// the rest is the TUI sidebar (source, symbol, price, period, year, sort).
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../state.dart';
import '../theme/muninn_theme.dart';

Future<void> showSourceFilter(BuildContext context, WidgetRef ref) =>
    _pick(context, ref, 'Source', (d) => d.source, (f) => f.sources, (v) {
      final cur = ref.read(dataFiltersProvider);
      ref.read(dataFiltersProvider.notifier).state = DataFilters(
        sources: v,
        symbols: cur.symbols,
        durations: cur.durations,
        years: cur.years,
        priceMetric: cur.priceMetric,
        priceMin: cur.priceMin,
        priceMax: cur.priceMax,
        sort: cur.sort,
        desc: cur.desc,
      );
    });

Future<void> showSymbolFilter(BuildContext context, WidgetRef ref) =>
    _pick(context, ref, 'Symbol', (d) => d.symbol, (f) => f.symbols, (v) {
      final cur = ref.read(dataFiltersProvider);
      ref.read(dataFiltersProvider.notifier).state = DataFilters(
        sources: cur.sources,
        symbols: v,
        durations: cur.durations,
        years: cur.years,
        priceMetric: cur.priceMetric,
        priceMin: cur.priceMin,
        priceMax: cur.priceMax,
        sort: cur.sort,
        desc: cur.desc,
      );
    });

Future<void> showPeriodFilter(BuildContext context, WidgetRef ref) =>
    _pick(context, ref, 'Period', (d) => d.duration, (f) => f.durations, (v) {
      final cur = ref.read(dataFiltersProvider);
      ref.read(dataFiltersProvider.notifier).state = DataFilters(
        sources: cur.sources,
        symbols: cur.symbols,
        durations: v,
        years: cur.years,
        priceMetric: cur.priceMetric,
        priceMin: cur.priceMin,
        priceMax: cur.priceMax,
        sort: cur.sort,
        desc: cur.desc,
      );
    });

Future<void> showYearFilter(BuildContext context, WidgetRef ref) async {
  final sets = ref.read(overviewProvider).value?.datasets ?? const <Dataset>[];
  final years = <int>{};
  for (final d in sets) {
    final a = DataFilters.yearOf(d.first);
    final b = DataFilters.yearOf(d.last);
    if (b - a > 15) {
      years.addAll([a, b]);
    } else {
      for (var y = a; y <= b; y++) {
        years.add(y);
      }
    }
  }
  final choices = years.toList()..sort();
  await _pick(
    context,
    ref,
    'Year',
    null,
    (f) => f.years.map((y) => '$y').toList(),
    (v) {
      final cur = ref.read(dataFiltersProvider);
      ref.read(dataFiltersProvider.notifier).state = DataFilters(
        sources: cur.sources,
        symbols: cur.symbols,
        durations: cur.durations,
        years: [for (final s in v) int.parse(s)],
        priceMetric: cur.priceMetric,
        priceMin: cur.priceMin,
        priceMax: cur.priceMax,
        sort: cur.sort,
        desc: cur.desc,
      );
    },
    choices: choices.map((y) => '$y').toList(),
  );
}

Future<void> _pick(
  BuildContext context,
  WidgetRef ref,
  String title,
  String Function(Dataset)? of,
  List<String> Function(DataFilters) selected,
  void Function(List<String>) apply, {
  List<String>? choices,
}) async {
  final sets = ref.read(overviewProvider).value?.datasets ?? const <Dataset>[];
  final known = choices ?? {for (final d in sets) of!(d)}.toList()
    ..sort();
  final current = [...selected(ref.read(dataFiltersProvider))];
  final extra = TextEditingController();
  final chosen = await showDialog<List<String>>(
    context: context,
    builder: (ctx) => StatefulBuilder(
      builder: (ctx, set) {
        final m = ctx.muninn;
        return AlertDialog(
          title: Text(title, style: TextStyle(color: m.header)),
          content: SizedBox(
            width: 360,
            height: 360,
            child: Column(
              children: [
                Text(
                  'Nothing checked shows every dataset.',
                  style: TextStyle(color: m.muted),
                ),
                Expanded(
                  child: ListView(
                    children: [
                      for (final c in {...known, ...current})
                        CheckboxListTile(
                          key: ValueKey('filter-$title-$c'),
                          dense: true,
                          value: current.contains(c),
                          title: Text(c),
                          onChanged: (on) => set(() {
                            if (on == true) {
                              current.add(c);
                            } else {
                              current.remove(c);
                            }
                          }),
                        ),
                    ],
                  ),
                ),
                TextField(
                  key: ValueKey('filter-$title-add'),
                  controller: extra,
                  decoration: const InputDecoration(labelText: 'Add a value'),
                  onSubmitted: (s) {
                    final v = s.trim();
                    if (v.isEmpty) return;
                    set(() => current.add(v));
                    extra.clear();
                  },
                ),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(ctx),
              child: const Text('Cancel'),
            ),
            FilledButton(
              key: ValueKey('filter-$title-apply'),
              onPressed: () => Navigator.pop(ctx, [...current]),
              child: const Text('Apply'),
            ),
          ],
        );
      },
    ),
  );
  extra.dispose();
  if (chosen != null) apply(chosen);
}

Future<void> showPriceFilter(BuildContext context, WidgetRef ref) async {
  final cur = ref.read(dataFiltersProvider);
  final min = TextEditingController(text: cur.priceMin?.toString() ?? '');
  final max = TextEditingController(text: cur.priceMax?.toString() ?? '');
  var metric = cur.priceMetric;
  final ok = await showDialog<bool>(
    context: context,
    builder: (ctx) => StatefulBuilder(
      builder: (ctx, set) {
        final m = ctx.muninn;
        return AlertDialog(
          title: Text('Price', style: TextStyle(color: m.header)),
          content: SizedBox(
            width: 360,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                DropdownButton<String>(
                  key: const ValueKey('filter-price-metric'),
                  isExpanded: true,
                  value: metric,
                  items: [
                    for (final name in DataFilters.priceMetrics)
                      DropdownMenuItem(value: name, child: Text(name)),
                  ],
                  onChanged: (v) => set(() => metric = v ?? metric),
                ),
                TextField(
                  key: const ValueKey('filter-price-min'),
                  controller: min,
                  decoration: const InputDecoration(labelText: 'Min'),
                ),
                TextField(
                  key: const ValueKey('filter-price-max'),
                  controller: max,
                  decoration: const InputDecoration(labelText: 'Max'),
                ),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(ctx, false),
              child: const Text('Cancel'),
            ),
            FilledButton(
              key: const ValueKey('filter-price-apply'),
              onPressed: () => Navigator.pop(ctx, true),
              child: const Text('Apply'),
            ),
          ],
        );
      },
    ),
  );
  if (ok == true) {
    double? n(String s) => s.trim().isEmpty ? null : double.tryParse(s.trim());
    ref.read(dataFiltersProvider.notifier).state = DataFilters(
      sources: cur.sources,
      symbols: cur.symbols,
      durations: cur.durations,
      years: cur.years,
      priceMetric: metric,
      priceMin: n(min.text),
      priceMax: n(max.text),
      sort: cur.sort,
      desc: cur.desc,
    );
  }
  min.dispose();
  max.dispose();
}

void setSort(WidgetRef ref, {String? sort, bool? desc}) {
  final cur = ref.read(dataFiltersProvider);
  ref.read(dataFiltersProvider.notifier).state = DataFilters(
    sources: cur.sources,
    symbols: cur.symbols,
    durations: cur.durations,
    years: cur.years,
    priceMetric: cur.priceMetric,
    priceMin: cur.priceMin,
    priceMax: cur.priceMax,
    sort: sort ?? cur.sort,
    desc: desc ?? cur.desc,
  );
}

Future<void> showCoverage(BuildContext context, WidgetRef ref) async {
  final rows = await ref.read(apiProvider).coverage();
  if (!context.mounted) return;
  final m = context.muninn;
  await showDialog<void>(
    context: context,
    builder: (ctx) => AlertDialog(
      title: Text('Coverage & gaps', style: TextStyle(color: m.header)),
      content: SizedBox(
        width: 520,
        child: rows.isEmpty
            ? const Text('No series yet.')
            : ListView(
                shrinkWrap: true,
                children: [
                  for (final r in rows)
                    ListTile(
                      dense: true,
                      title: Text('${r.source} ${r.symbol} ${r.duration}'),
                      subtitle: Text(
                        '${r.bars} bars, ${r.missing} missing · ${r.first} → ${r.last}',
                      ),
                    ),
                ],
              ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(ctx),
          child: const Text('Close'),
        ),
      ],
    ),
  );
}
