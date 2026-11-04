/// Home: where the data lives, and (after unlocking) projects and datasets, read-only.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../state.dart';
import '../theme/muninn_theme.dart';

class HomeView extends ConsumerWidget {
  const HomeView({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final dir = ref.watch(dataDirProvider);
    final overview = ref.watch(overviewProvider);
    return ListView(padding: const EdgeInsets.all(16), children: [
      dir.when(
        data: (d) => _DataDirCard(d),
        loading: () => const LinearProgressIndicator(),
        error: (e, _) => Text('Data dir: $e'),
      ),
      const SizedBox(height: 12),
      overview.when(
        data: (o) => o == null ? const _Unlock() : _OverviewBody(o),
        loading: () => const LinearProgressIndicator(),
        error: (e, _) => Column(children: [Text('$e', style: TextStyle(color: context.muninn.error)), const _Unlock()]),
      ),
    ]);
  }
}

class _DataDirCard extends StatelessWidget {
  const _DataDirCard(this.d);
  final DataDir d;

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return Card(
      child: ListTile(
        leading: Icon(Icons.storage, color: m.accent),
        title: Text(d.home),
        subtitle: Text(d.databaseExists
            ? (d.keysInitialized ? 'Encrypted store found (shared with mnnn)' : 'Store found, keys not initialized: run `mnnn keys init`')
            : 'No store yet: run `mnnn keys init`, then import with `mnnn ingest path DIR`'),
      ),
    );
  }
}

class _Unlock extends ConsumerStatefulWidget {
  const _Unlock();
  @override
  ConsumerState<_Unlock> createState() => _UnlockState();
}

class _UnlockState extends ConsumerState<_Unlock> {
  final _c = TextEditingController();

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  void _go() {
    if (_c.text.isNotEmpty) ref.read(passwordProvider.notifier).state = _c.text;
  }

  @override
  Widget build(BuildContext context) => Card(
        child: Padding(
          padding: const EdgeInsets.all(16),
          child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            Text('Unlock to browse (read-only)', style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            TextField(
              controller: _c,
              obscureText: true,
              autofillHints: const [AutofillHints.password],
              decoration: const InputDecoration(labelText: 'Muninn password', border: OutlineInputBorder()),
              onSubmitted: (_) => _go(),
            ),
            const SizedBox(height: 8),
            FilledButton(onPressed: _go, style: FilledButton.styleFrom(minimumSize: const Size.fromHeight(48)), child: const Text('Unlock')),
          ]),
        ),
      );
}

class _OverviewBody extends StatelessWidget {
  const _OverviewBody(this.o);
  final Overview o;

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    String day(int ms) => DateTime.fromMillisecondsSinceEpoch(ms, isUtc: true).toIso8601String().substring(0, 10);
    Widget heading(String t, int n) => Padding(
          padding: const EdgeInsets.fromLTRB(4, 16, 4, 8),
          child: Text('$t ($n)', style: TextStyle(color: m.header, fontSize: 22, fontWeight: FontWeight.w800)),
        );
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      heading('Projects', o.projects.length),
      if (o.projects.isEmpty) Text('No projects: `mnnn project create NAME`', style: TextStyle(color: m.muted)),
      for (final p in o.projects)
        Card(child: ListTile(leading: Icon(Icons.folder, color: m.accent), title: Text(p.name), subtitle: Text(p.description), trailing: Text(p.shortId))),
      heading('Datasets', o.datasets.length),
      if (o.datasets.isEmpty) Text('No bars yet: `mnnn ingest path DIR`', style: TextStyle(color: m.muted)),
      for (final d in o.datasets)
        Card(
          child: ListTile(
            leading: Icon(Icons.candlestick_chart, color: m.up),
            title: Text('${d.source} · ${d.symbol} · ${d.duration}'),
            subtitle: Text('${d.count} bars · ${day(d.first)} → ${day(d.last)}'),
          ),
        ),
    ]);
  }
}
