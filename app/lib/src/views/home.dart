/// Home: where the data lives, and (after unlocking) projects and datasets.
/// Tapping a project opens it in Projects; tapping a dataset opens it in the
/// Data workspace (the first tile), like picking a series in the TUI sidebar.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../state.dart';
import 'keys_dialogs.dart';
import '../theme/muninn_theme.dart';
import '../workspace.dart';
import 'unlock.dart';

class HomeView extends ConsumerWidget {
  const HomeView({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final dir = ref.watch(dataDirProvider);
    final overview = ref.watch(overviewProvider);
    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        dir.when(
          data: (d) => _DataDirCard(d),
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Text('Data dir: $e'),
        ),
        const SizedBox(height: 12),
        overview.when(
          data: (o) => o == null ? const UnlockCard() : _OverviewBody(o),
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Column(
            children: [
              Text('$e', style: TextStyle(color: context.muninn.error)),
              const UnlockCard(),
            ],
          ),
        ),
      ],
    );
  }
}

class _DataDirCard extends ConsumerWidget {
  const _DataDirCard(this.d);
  final DataDir d;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final ready = d.keysInitialized;
    return Card(
      child: ListTile(
        leading: Icon(Icons.storage, color: m.accent),
        title: Text(d.home),
        subtitle: Text(
          d.databaseExists
              ? (ready
                    ? 'Encrypted store found (shared with mnnn)'
                    : 'Store found, keys not initialized')
              : 'No store yet: initialize keys, then import with File ▸ Import… or `mnnn ingest path DIR`',
        ),
        trailing: ready
            ? null
            : TextButton(
                key: const ValueKey('home-keys-init'),
                onPressed: () {
                  ref.read(sectionProvider.notifier).state = Section.keys;
                  showKeysInitDialog(context, ref);
                },
                child: const Text('Initialize…'),
              ),
      ),
    );
  }
}

class _OverviewBody extends ConsumerWidget {
  const _OverviewBody(this.o);
  final Overview o;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    String day(int ms) => DateTime.fromMillisecondsSinceEpoch(
      ms,
      isUtc: true,
    ).toIso8601String().substring(0, 10);
    Widget heading(String t, int n) => Padding(
      padding: const EdgeInsets.fromLTRB(4, 16, 4, 8),
      child: Text(
        '$t ($n)',
        style: TextStyle(
          color: m.header,
          fontSize: 22,
          fontWeight: FontWeight.w800,
        ),
      ),
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        heading('Projects', o.projects.length),
        if (o.projects.isEmpty)
          Text(
            'No projects: `mnnn project create NAME`',
            style: TextStyle(color: m.muted),
          ),
        for (final p in o.projects)
          Card(
            child: ListTile(
              key: ValueKey('home-project-${p.id}'),
              leading: Icon(Icons.folder, color: m.accent),
              title: Text(p.name),
              subtitle: Text(p.description),
              trailing: Text(p.shortId),
              onTap: () {
                ref.read(selectedProjectProvider.notifier).state = p;
                ref.read(sectionProvider.notifier).state = Section.projects;
              },
            ),
          ),
        heading('Datasets', o.datasets.length),
        if (o.datasets.isEmpty)
          Text(
            'No bars yet: `mnnn ingest path DIR`',
            style: TextStyle(color: m.muted),
          ),
        for (final d in o.datasets)
          Card(
            child: ListTile(
              key: ValueKey('home-dataset-${d.key}'),
              leading: Icon(Icons.candlestick_chart, color: m.up),
              title: Text('${d.source} · ${d.symbol} · ${d.duration}'),
              subtitle: Text(
                '${d.count} bars · ${day(d.first)} → ${day(d.last)}',
              ),
              onTap: () {
                ref.read(selectedDatasetProvider.notifier).state = d.key;
                setWorkspace(
                  ref,
                  ref
                      .read(workspaceProvider)
                      .setFocusedDataset(d.key)
                      .unfocusSidebar(),
                );
                ref.read(sectionProvider.notifier).state = Section.data;
              },
            ),
          ),
      ],
    );
  }
}
