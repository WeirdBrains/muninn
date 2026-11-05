/// Projects (read-only): pick one to see its saved series; opening a series shows it in
/// Data with the project's saved time range.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../state.dart';
import '../theme/muninn_theme.dart';
import '../time_range.dart';
import 'unlock.dart';

class ProjectsView extends ConsumerWidget {
  const ProjectsView({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final sel = ref.watch(selectedProjectProvider);
    return Unlocked(
      child: ref.watch(overviewProvider).when(
            loading: () => const LinearProgressIndicator(),
            error: (e, _) => Text('$e', style: TextStyle(color: m.error)),
            data: (o) {
              final ps = o?.projects ?? const <Project>[];
              if (ps.isEmpty) {
                return Padding(padding: const EdgeInsets.all(16), child: Text('No projects: `mnnn project create NAME`', style: TextStyle(color: m.muted)));
              }
              return ListView(padding: const EdgeInsets.all(8), children: [
                for (final p in ps) ...[
                  ListTile(
                    minTileHeight: 56,
                    leading: Icon(Icons.folder, color: m.accent),
                    title: Text(p.name),
                    subtitle: p.description.isEmpty ? null : Text(p.description),
                    trailing: Icon(sel?.id == p.id ? Icons.expand_less : Icons.expand_more),
                    onTap: () => ref.read(selectedProjectProvider.notifier).state = sel?.id == p.id ? null : p,
                  ),
                  if (sel?.id == p.id) _ProjectSeriesList(o!),
                ],
              ]);
            },
          ),
    );
  }
}

class _ProjectSeriesList extends ConsumerWidget {
  const _ProjectSeriesList(this.o);
  final Overview o;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return ref.watch(projectSeriesProvider).when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Text('$e', style: TextStyle(color: m.error)),
          data: (items) {
            if (items == null || items.isEmpty) {
              return Padding(padding: const EdgeInsets.fromLTRB(72, 0, 16, 12), child: Text('No series in this project yet', style: TextStyle(color: m.muted)));
            }
            return Column(children: [
              for (final s in items)
                ListTile(
                  minTileHeight: 48,
                  contentPadding: const EdgeInsets.only(left: 72, right: 16),
                  leading: Icon(Icons.candlestick_chart, color: m.up),
                  title: Text('${s.source} · ${s.base}/${s.quote}'),
                  subtitle: Text(s.from == null && s.to == null
                      ? 'all time'
                      : '${s.from == null ? '…' : fmtUtc(s.from!)} → ${s.to == null ? '…' : fmtUtc(s.to!)} UTC'),
                  onTap: () {
                    ref.read(timeRangeProvider.notifier).state = TimeRange(from: s.from, to: s.to);
                    ref.read(selectedDatasetProvider.notifier).state = s.key;
                    ref.read(sectionProvider.notifier).state = Section.data;
                  },
                ),
            ]);
          },
        );
  }
}
