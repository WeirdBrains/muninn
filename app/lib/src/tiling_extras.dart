/// Saved layouts, the history bar (`Y` / `u` / `U`), File ▸ Authors… (`A`),
/// per-tile layers/viewport helpers, linked list↔chart pan, and found-views.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/legacy.dart';

import 'api.dart';
import 'state.dart';
import 'theme/muninn_theme.dart';
import 'workspace.dart';

final historyOpenProvider = StateProvider<bool>((ref) => false);
final historyTickProvider = StateProvider<int>((ref) => 0);

final historyRowsProvider = FutureProvider<List<HistoryRowInfo>>((ref) async {
  ref.watch(historyTickProvider);
  if (!ref.watch(unlockedProvider)) return const [];
  return ref.watch(apiProvider).historyTree();
});

void bumpHistory(WidgetRef ref) =>
    ref.read(historyTickProvider.notifier).state++;

void applyFocusedLayers(WidgetRef ref, ChartLayers next) {
  ref.read(chartLayersProvider.notifier).state = next;
  final ws = ref.read(workspaceProvider);
  setWorkspace(ref, ws.updateTile(ws.focus, (t) => t.copyWith(layers: next)));
}

void applyFocusedViewport(WidgetRef ref, ChartViewport next, {int? tileId}) {
  ref.read(chartViewportProvider.notifier).state = next;
  final ws = ref.read(workspaceProvider);
  final id = tileId ?? ws.focus;
  setWorkspace(ref, ws.updateTile(id, (t) => t.copyWith(viewport: next)));
}

void syncLinkedFromList(WidgetRef ref, int tileId, int ts) {
  final ws = ref.read(workspaceProvider);
  var next = ws.updateTile(tileId, (t) => t.copyWith(listCursorTs: ts));
  for (final t in ws.linkedTo(tileId)) {
    if (t.asChart) {
      var vp = t.viewport;
      final key = t.datasetKey;
      if (key != null) {
        final bars = ref.read(tileBarsProvider(key)).value;
        if (bars != null && bars.isNotEmpty) {
          vp = vp.jumpTo(nearestBarIndex(bars, ts), bars.length);
        }
      }
      next = next.updateTile(
        t.id,
        (x) => x.copyWith(listCursorTs: ts, viewport: vp),
      );
    } else {
      next = next.updateTile(t.id, (x) => x.copyWith(listCursorTs: ts));
    }
  }
  setWorkspace(ref, next);
}

void syncLinkedFromChart(
  WidgetRef ref,
  int tileId,
  ChartViewport vp,
  List<BarPoint> bars,
) {
  final ws = ref.read(workspaceProvider);
  var next = ws.updateTile(tileId, (t) => t.copyWith(viewport: vp));
  if (bars.isNotEmpty) {
    final n = bars.length;
    final i = (vp.visibleStart(n) + vp.visibleCount(n) ~/ 2).clamp(0, n - 1);
    final ts = bars[i].ts;
    for (final t in ws.linkedTo(tileId)) {
      if (!t.asChart) {
        next = next.updateTile(t.id, (x) => x.copyWith(listCursorTs: ts));
      }
    }
  }
  setWorkspace(ref, next);
}

void applyHistoryMove(WidgetRef ref, HistoryMoveInfo m) {
  if (m.theme.isNotEmpty) {
    ref.read(themeSelectionProvider.notifier).set(m.theme);
  }
  if (m.mode.isNotEmpty) {
    ref.read(themeSelectionProvider.notifier).setMode(m.mode);
  }
  if (m.rangeFrom != null || m.rangeTo != null) {
    ref.read(timeRangeProvider.notifier).state = TimeRange(
      from: m.rangeFrom,
      to: m.rangeTo,
    );
  }
  bumpHistory(ref);
}

Future<void> toggleHistory(WidgetRef ref) async {
  ref.read(historyOpenProvider.notifier).state = !ref.read(historyOpenProvider);
}

Future<void> stepHistory(WidgetRef ref, {required bool back}) async {
  try {
    final m = back
        ? await ref.read(apiProvider).historyBack()
        : await ref.read(apiProvider).historyForward();
    applyHistoryMove(ref, m);
  } catch (e) {
    bumpHistory(ref);
    rethrow;
  }
}

class HistoryPane extends ConsumerWidget {
  const HistoryPane({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return DecoratedBox(
      decoration: BoxDecoration(
        border: Border(right: BorderSide(color: m.border)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(12, 10, 8, 4),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    'History',
                    style: TextStyle(
                      color: m.header,
                      fontWeight: FontWeight.w800,
                    ),
                  ),
                ),
                IconButton(
                  key: const ValueKey('history-back'),
                  tooltip: 'Step back (u)',
                  icon: const Icon(Icons.undo),
                  onPressed: () => _step(context, ref, back: true),
                ),
                IconButton(
                  key: const ValueKey('history-forward'),
                  tooltip: 'Step forward (U)',
                  icon: const Icon(Icons.redo),
                  onPressed: () => _step(context, ref, back: false),
                ),
                IconButton(
                  tooltip: 'Hide history',
                  icon: const Icon(Icons.close),
                  onPressed: () => toggleHistory(ref),
                ),
              ],
            ),
          ),
          Expanded(
            child: ref
                .watch(historyRowsProvider)
                .when(
                  loading: () => const LinearProgressIndicator(),
                  error: (e, _) => Padding(
                    padding: const EdgeInsets.all(12),
                    child: Text('$e', style: TextStyle(color: m.error)),
                  ),
                  data: (rows) => ListView(
                    children: [
                      ListTile(
                        key: const ValueKey('history-row-start'),
                        minTileHeight: 48,
                        dense: true,
                        title: const Text('(start)'),
                        onTap: () => _goto(context, ref, null),
                      ),
                      for (final r in rows)
                        ListTile(
                          key: ValueKey('history-row-${r.id}'),
                          minTileHeight: 48,
                          dense: true,
                          selected: r.head || r.current,
                          selectedTileColor: m.selectionBg,
                          selectedColor: m.selectionFg,
                          title: Text(
                            '${'  ' * r.depth}${r.fork ? '↳ ' : ''}${r.summary}',
                          ),
                          subtitle: Text(r.kind),
                          onTap: () => _goto(context, ref, r.id),
                        ),
                    ],
                  ),
                ),
          ),
        ],
      ),
    );
  }

  Future<void> _step(
    BuildContext context,
    WidgetRef ref, {
    required bool back,
  }) async {
    try {
      await stepHistory(ref, back: back);
    } catch (e) {
      if (context.mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text('$e')));
      }
    }
  }

  Future<void> _goto(BuildContext context, WidgetRef ref, int? id) async {
    try {
      final m = await ref.read(apiProvider).historyGoto(id);
      applyHistoryMove(ref, m);
    } catch (e) {
      if (context.mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text('$e')));
      }
    }
  }
}

class _SaveLayoutDialog extends StatefulWidget {
  const _SaveLayoutDialog();
  @override
  State<_SaveLayoutDialog> createState() => _SaveLayoutDialogState();
}

class _SaveLayoutDialogState extends State<_SaveLayoutDialog> {
  final name = TextEditingController();
  @override
  void dispose() {
    name.dispose();
    super.dispose();
  }

  void _submit() => Navigator.pop(context, name.text.trim());

  @override
  Widget build(BuildContext context) => AlertDialog(
    key: const ValueKey('layout-save-dialog'),
    title: const Text('Save current layout'),
    content: TextField(
      key: const ValueKey('layout-save-name'),
      controller: name,
      autofocus: true,
      decoration: const InputDecoration(labelText: 'Name'),
      onSubmitted: (_) => _submit(),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        key: const ValueKey('layout-save-confirm'),
        onPressed: _submit,
        child: const Text('Save'),
      ),
    ],
  );
}

Future<void> showSaveLayoutDialog(BuildContext context, WidgetRef ref) async {
  final n = await showDialog<String>(
    context: context,
    builder: (ctx) => const _SaveLayoutDialog(),
  );
  if (n == null || n.isEmpty || !context.mounted) return;
  try {
    final msg = await ref
        .read(apiProvider)
        .saveNamedLayout(n, ref.read(workspaceProvider).encode());
    if (context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(msg)));
    }
  } catch (e) {
    if (context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text('$e')));
    }
  }
}

Future<void> showSavedLayoutsDialog(BuildContext context, WidgetRef ref) async {
  List<String> names;
  try {
    names = await ref.read(apiProvider).savedLayouts();
  } catch (e) {
    if (context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text('$e')));
    }
    return;
  }
  if (!context.mounted) return;
  await showDialog<void>(
    context: context,
    builder: (ctx) => AlertDialog(
      key: const ValueKey('layout-saved-dialog'),
      title: const Text('Saved layouts'),
      content: SizedBox(
        width: 420,
        height: 320,
        child: names.isEmpty
            ? const Text('No saved layouts yet')
            : ListView(
                children: [
                  for (final n in names)
                    ListTile(
                      key: ValueKey('layout-item-$n'),
                      minTileHeight: 56,
                      title: Text(n),
                      onTap: () async {
                        try {
                          final json = await ref
                              .read(apiProvider)
                              .namedLayout(n);
                          setWorkspace(ref, Workspace.decode(json));
                          if (ctx.mounted) Navigator.pop(ctx);
                        } catch (e) {
                          if (ctx.mounted) {
                            ScaffoldMessenger.of(ctx)
                                .showSnackBar(SnackBar(content: Text('$e')));
                          }
                        }
                      },
                      trailing: IconButton(
                        key: ValueKey('layout-remove-$n'),
                        tooltip: 'Remove',
                        icon: const Icon(Icons.delete_outline),
                        onPressed: () async {
                          try {
                            await ref.read(apiProvider).removeNamedLayout(n);
                            if (ctx.mounted) Navigator.pop(ctx);
                            if (context.mounted) {
                              await showSavedLayoutsDialog(context, ref);
                            }
                          } catch (e) {
                            if (ctx.mounted) {
                              ScaffoldMessenger.of(ctx)
                                  .showSnackBar(SnackBar(content: Text('$e')));
                            }
                          }
                        },
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

class _AuthorEditResult {
  const _AuthorEditResult(this.name, this.email, this.details);
  final String name;
  final String email;
  final List<String> details;
}

class _AuthorEditDialog extends StatefulWidget {
  const _AuthorEditDialog({this.existing});
  final AuthorInfo? existing;
  @override
  State<_AuthorEditDialog> createState() => _AuthorEditDialogState();
}

class _AuthorEditDialogState extends State<_AuthorEditDialog> {
  late final TextEditingController name;
  late final TextEditingController email;
  late final TextEditingController details;

  @override
  void initState() {
    super.initState();
    name = TextEditingController(text: widget.existing?.name ?? '');
    email = TextEditingController(text: widget.existing?.email ?? '');
    details = TextEditingController(
      text: widget.existing?.details.join('\n') ?? '',
    );
  }

  @override
  void dispose() {
    name.dispose();
    email.dispose();
    details.dispose();
    super.dispose();
  }

  void _submit() {
    Navigator.pop(
      context,
      _AuthorEditResult(name.text, email.text, [
        for (final line in details.text.split('\n'))
          if (line.trim().isNotEmpty) line.trim(),
      ]),
    );
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    key: const ValueKey('author-edit-dialog'),
    title: Text(widget.existing == null ? 'Add author' : 'Edit author'),
    content: SizedBox(
      width: 420,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          TextField(
            key: const ValueKey('author-name'),
            controller: name,
            autofocus: true,
            decoration: const InputDecoration(labelText: 'Name'),
          ),
          TextField(
            key: const ValueKey('author-email'),
            controller: email,
            decoration: const InputDecoration(labelText: 'Email'),
          ),
          TextField(
            key: const ValueKey('author-details'),
            controller: details,
            decoration: const InputDecoration(
              labelText: 'Details (key=value, one per line)',
            ),
            minLines: 2,
            maxLines: 4,
          ),
        ],
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        key: const ValueKey('author-save'),
        onPressed: _submit,
        child: const Text('Save'),
      ),
    ],
  );
}

Future<void> showAuthorsDialog(BuildContext context, WidgetRef ref) async {
  await showDialog<void>(
    context: context,
    builder: (ctx) => const _AuthorsDialog(),
  );
}

class _AuthorsDialog extends ConsumerStatefulWidget {
  const _AuthorsDialog();
  @override
  ConsumerState<_AuthorsDialog> createState() => _AuthorsDialogState();
}

class _AuthorsDialogState extends ConsumerState<_AuthorsDialog> {
  List<AuthorInfo> items = const [];
  String? error;
  bool loading = true;

  @override
  void initState() {
    super.initState();
    _reload();
  }

  Future<void> _reload() async {
    setState(() {
      loading = true;
      error = null;
    });
    try {
      final next = await ref.read(apiProvider).authors();
      if (mounted) {
        setState(() {
          items = next;
          loading = false;
        });
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          error = '$e';
          loading = false;
        });
      }
    }
  }

  Future<void> _run(Future<void> Function() f) async {
    try {
      await f();
      await _reload();
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    }
  }

  Future<void> _edit({AuthorInfo? existing}) async {
    final result = await showDialog<_AuthorEditResult>(
      context: context,
      builder: (ctx) => _AuthorEditDialog(existing: existing),
    );
    if (result == null) return;
    final n = result.name;
    final e = result.email;
    final d = result.details;
    await _run(() async {
      if (existing == null) {
        await ref
            .read(apiProvider)
            .addAuthor(
              n,
              email: e.trim().isEmpty ? null : e.trim(),
              details: d,
            );
      } else {
        await ref
            .read(apiProvider)
            .updateAuthor(
              existing.id,
              name: n,
              email: e,
              details: d,
              replaceDetails: true,
            );
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      key: const ValueKey('authors-dialog'),
      title: const Text('Authors'),
      content: SizedBox(
        width: 480,
        height: 360,
        child: loading
            ? const Center(child: CircularProgressIndicator())
            : Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  if (error != null)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 8),
                      child: Text(error!, style: TextStyle(color: m.error)),
                    ),
                  Expanded(
                    child: items.isEmpty
                        ? Text(
                            'No authors yet',
                            style: TextStyle(color: m.muted),
                          )
                        : ListView(
                            children: [
                              for (final a in items)
                                ListTile(
                                  key: ValueKey('author-${a.id}'),
                                  minTileHeight: 56,
                                  leading: Icon(
                                    a.current
                                        ? Icons.person
                                        : Icons.person_outline,
                                    color: a.current ? m.accent : m.muted,
                                  ),
                                  title: Text(a.name),
                                  subtitle: Text(
                                    [
                                      if (a.email.isNotEmpty) a.email,
                                      ...a.details,
                                    ].join(' · '),
                                  ),
                                  trailing: PopupMenuButton<String>(
                                    key: ValueKey('author-menu-${a.id}'),
                                    onSelected: (v) {
                                      switch (v) {
                                        case 'current':
                                          _run(
                                            () => ref
                                                .read(apiProvider)
                                                .setCurrentAuthor(a.id),
                                          );
                                        case 'edit':
                                          _edit(existing: a);
                                        case 'delete':
                                          _run(
                                            () => ref
                                                .read(apiProvider)
                                                .removeAuthor(a.id),
                                          );
                                      }
                                    },
                                    itemBuilder: (ctx) => const [
                                      PopupMenuItem(
                                        value: 'current',
                                        child: Text('Make current'),
                                      ),
                                      PopupMenuItem(
                                        value: 'edit',
                                        child: Text('Edit…'),
                                      ),
                                      PopupMenuItem(
                                        value: 'delete',
                                        child: Text('Delete'),
                                      ),
                                    ],
                                  ),
                                ),
                            ],
                          ),
                  ),
                ],
              ),
      ),
      actions: [
        TextButton(
          key: const ValueKey('authors-add'),
          onPressed: () => _edit(),
          child: const Text('Add…'),
        ),
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Close'),
        ),
      ],
    );
  }
}

Future<void> showFoundEntriesDialog(
  BuildContext context,
  WidgetRef ref,
  SearchInfo s,
) async {
  List<FoundEntryInfo> hits;
  try {
    hits = await ref
        .read(apiProvider)
        .searchFoundEntries(s.query, from: s.from, to: s.to);
  } catch (e) {
    if (context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text('$e')));
    }
    return;
  }
  if (!context.mounted) return;
  await showDialog<void>(
    context: context,
    builder: (ctx) => AlertDialog(
      key: const ValueKey('found-entries-dialog'),
      title: Text('Found entries · ${s.name}'),
      content: SizedBox(
        width: 520,
        height: 360,
        child: hits.isEmpty
            ? const Text('No matching journal entries')
            : ListView(
                children: [
                  for (final e in hits)
                    ListTile(
                      key: ValueKey('found-entry-${e.entryId}'),
                      minTileHeight: 56,
                      title: Text(e.heading.isNotEmpty ? e.heading : e.title),
                      subtitle: Text(e.journalName),
                      onTap: () async {
                        try {
                          final journals = await ref
                              .read(apiProvider)
                              .journals();
                          JournalInfo? j;
                          for (final x in journals) {
                            if (x.id == e.journalId) j = x;
                          }
                          ref.read(sectionProvider.notifier).state =
                              Section.projects;
                          ref.read(projectPaneProvider.notifier).state =
                              ProjectPane.journals;
                          if (j != null) {
                            ref.read(selectedJournalProvider.notifier).state =
                                j;
                          }
                          if (ctx.mounted) Navigator.pop(ctx);
                        } catch (err) {
                          if (ctx.mounted) {
                            ScaffoldMessenger.of(ctx)
                                .showSnackBar(SnackBar(content: Text('$err')));
                          }
                        }
                      },
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

Future<void> showFoundRecordsDialog(
  BuildContext context,
  WidgetRef ref,
  SearchInfo s,
) async {
  List<FoundRecordInfo> hits;
  try {
    hits = await ref
        .read(apiProvider)
        .searchFoundRecords(s.query, from: s.from, to: s.to);
  } catch (e) {
    if (context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text('$e')));
    }
    return;
  }
  if (!context.mounted) return;
  await showDialog<void>(
    context: context,
    builder: (ctx) => AlertDialog(
      key: const ValueKey('found-records-dialog'),
      title: Text('Found records · ${s.name}'),
      content: SizedBox(
        width: 520,
        height: 360,
        child: hits.isEmpty
            ? const Text('No matching records')
            : ListView(
                children: [
                  for (final r in hits)
                    ListTile(
                      key: ValueKey('found-record-${r.recordId}'),
                      minTileHeight: 56,
                      title: Text(r.brief.isNotEmpty ? r.brief : r.recordId),
                      subtitle: Text(r.modelName),
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
