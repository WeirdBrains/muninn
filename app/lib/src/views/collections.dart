/// Code spaces as collections (`mnnn spaces`) and `mnnn migrate`.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../state.dart';
import '../theme/muninn_theme.dart';

void _ok(BuildContext context, String msg, {Color? color}) {
  ScaffoldMessenger.of(context)
      .showSnackBar(SnackBar(content: Text(msg), backgroundColor: color));
}

Future<bool> _confirm(BuildContext context, String title, String body) async {
  final m = context.muninn;
  final yes = await showDialog<bool>(
    context: context,
    builder: (ctx) => AlertDialog(
      backgroundColor: m.bg,
      title: Text(title, style: TextStyle(color: m.header)),
      content: Text(body),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(ctx, false),
          child: const Text('Cancel'),
        ),
        FilledButton(
          onPressed: () => Navigator.pop(ctx, true),
          child: const Text('OK'),
        ),
      ],
    ),
  );
  return yes == true;
}

/// One-time offer when code is not in a space yet (TUI status line / Code ▸ Migrate…).
class MigrateBanner extends ConsumerWidget {
  const MigrateBanner({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    if (!ref.watch(unlockedProvider)) return const SizedBox.shrink();
    final pending = ref.watch(migratePendingProvider).value;
    final offered = ref.watch(migrateOfferedProvider).value ?? true;
    if (pending == null || pending <= 0 || offered) {
      return const SizedBox.shrink();
    }
    final m = context.muninn;
    return Material(
      color: m.accent.withValues(alpha: 0.12),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        child: Wrap(
          spacing: 8,
          runSpacing: 8,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            Text(
              '$pending code item(s) aren\'t in a code space yet. Migrate shows a dry run first; nothing changes until you confirm.',
              style: TextStyle(color: m.fg),
            ),
            FilledButton(
              key: const ValueKey('migrate-now'),
              onPressed: () => showMigrateDialog(context, ref),
              child: const Text('Migrate…'),
            ),
            TextButton(
              key: const ValueKey('migrate-later'),
              onPressed: () async {
                await ref.read(apiProvider).migrateDismiss();
                ref.invalidate(migrateOfferedProvider);
              },
              child: const Text('Later'),
            ),
          ],
        ),
      ),
    );
  }
}

Future<void> showMigrateDialog(BuildContext context, WidgetRef ref) async {
  if (!ref.read(unlockedProvider)) return;
  final report = await showDialog<MigrateReport>(
    context: context,
    builder: (ctx) => const _MigrateDialog(),
  );
  if (report != null) refreshProjectExtras(ref);
}

class CollectionsPane extends ConsumerWidget {
  const CollectionsPane({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return ref
        .watch(collectionsProvider)
        .when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Padding(
            padding: const EdgeInsets.all(16),
            child: Text('$e', style: TextStyle(color: m.error)),
          ),
          data: (items) {
            final all = items ?? const <CollectionInfo>[];
            return ListView(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
              children: [
                Text(
                  'Code spaces are collections of code. The main runs; members can be called from it. Approval covers the line-up.',
                  style: TextStyle(color: m.muted),
                ),
                const SizedBox(height: 12),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    FilledButton.icon(
                      key: const ValueKey('spaces-new'),
                      onPressed: () => showNewCollectionDialog(context, ref),
                      icon: const Icon(Icons.add),
                      label: const Text('New code space…'),
                    ),
                    OutlinedButton(
                      key: const ValueKey('spaces-migrate'),
                      onPressed: () => showMigrateDialog(context, ref),
                      child: const Text('Migrate…'),
                    ),
                  ],
                ),
                const SizedBox(height: 12),
                if (all.isEmpty)
                  Text(
                    'No code spaces yet. Migrate… turns each code into a space, or create one with a main.',
                    style: TextStyle(color: m.muted),
                  )
                else
                  for (final c in all)
                    ListTile(
                      key: ValueKey('space-${c.id}'),
                      contentPadding: EdgeInsets.zero,
                      title: Text(
                        '${c.approved ? '●' : '○'} ${c.name}${c.enabled ? '' : ' (off)'}',
                      ),
                      subtitle: Text(
                        [
                          c.kind,
                          if (c.main.isNotEmpty) 'main ${c.main}',
                          '${c.members} member(s)',
                          if (c.unreviewed > 0) '${c.unreviewed} unreviewed',
                          if (c.pinBroken > 0) '${c.pinBroken} pin changed',
                        ].join(' · '),
                        style: TextStyle(
                          color: c.unreviewed > 0 || c.pinBroken > 0
                              ? m.warning
                              : m.muted,
                        ),
                      ),
                      onTap: () => _members(context, ref, c),
                      trailing: PopupMenuButton<String>(
                        onSelected: (v) => _action(context, ref, c, v),
                        itemBuilder: (_) => [
                          const PopupMenuItem(
                            value: 'members',
                            child: Text('Members…'),
                          ),
                          const PopupMenuItem(value: 'run', child: Text('Run')),
                          const PopupMenuItem(
                            value: 'approve',
                            child: Text('Approve'),
                          ),
                          PopupMenuItem(
                            value: 'enable',
                            child: Text(c.enabled ? 'Disable' : 'Enable'),
                          ),
                          const PopupMenuItem(
                            value: 'remove',
                            child: Text('Remove'),
                          ),
                        ],
                      ),
                    ),
              ],
            );
          },
        );
  }
}

Future<void> _action(
  BuildContext context,
  WidgetRef ref,
  CollectionInfo c,
  String v,
) async {
  final m = context.muninn;
  try {
    switch (v) {
      case 'members':
        await _members(context, ref, c);
      case 'run':
        final r = await ref.read(apiProvider).collectionRun(c.id);
        if (context.mounted) {
          await _lines(context, '${c.name}: ${r.status}', [
            ...r.lines,
            if (r.error.isNotEmpty) r.error,
          ]);
        }
      case 'approve':
        if (!await _confirm(
          context,
          'Approve ${c.name}',
          'Approve this space\'s line-up and its unreviewed code? It runs as native code with your rights.',
        )) {
          return;
        }
        final msg = await ref.read(apiProvider).collectionApprove(c.id);
        refreshProjectExtras(ref);
        if (context.mounted) _ok(context, msg);
      case 'enable':
        final msg = await ref
            .read(apiProvider)
            .collectionSetEnabled(c.id, !c.enabled);
        refreshProjectExtras(ref);
        if (context.mounted) _ok(context, msg);
      case 'remove':
        if (!await _confirm(
          context,
          'Remove ${c.name}',
          'Remove this code space? Its code stays.',
        )) {
          return;
        }
        final msg = await ref.read(apiProvider).collectionRemove(c.id);
        refreshProjectExtras(ref);
        if (context.mounted) _ok(context, msg);
    }
  } catch (e) {
    if (context.mounted) _ok(context, '$e', color: m.error);
  }
}

Future<void> _lines(
  BuildContext context,
  String title,
  List<String> lines,
) async {
  final m = context.muninn;
  await showDialog<void>(
    context: context,
    builder: (ctx) => AlertDialog(
      backgroundColor: m.bg,
      title: Text(title, style: TextStyle(color: m.header)),
      content: SingleChildScrollView(
        child: Text(lines.isEmpty ? '(no output)' : lines.join('\n')),
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

Future<void> showNewCollectionDialog(
  BuildContext context,
  WidgetRef ref,
) async {
  if (!ref.read(unlockedProvider)) return;
  final ok = await showDialog<bool>(
    context: context,
    builder: (ctx) => const _NewSpaceDialog(),
  );
  if (ok == true) refreshProjectExtras(ref);
}

class _NewSpaceDialog extends ConsumerStatefulWidget {
  const _NewSpaceDialog();
  @override
  ConsumerState<_NewSpaceDialog> createState() => _NewSpaceDialogState();
}

class _NewSpaceDialogState extends ConsumerState<_NewSpaceDialog> {
  final _name = TextEditingController();
  final _about = TextEditingController();
  final _main = TextEditingController();
  var _kind = 'analysis';
  var _busy = false;

  @override
  void dispose() {
    _name.dispose();
    _about.dispose();
    _main.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text('New code space', style: TextStyle(color: m.header)),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              key: const ValueKey('space-name'),
              controller: _name,
              decoration: const InputDecoration(labelText: 'Name'),
            ),
            TextField(
              controller: _about,
              decoration: const InputDecoration(labelText: 'Description'),
            ),
            DropdownButtonFormField<String>(
              key: const ValueKey('space-kind'),
              initialValue: _kind,
              items: const [
                DropdownMenuItem(value: 'parser', child: Text('parser')),
                DropdownMenuItem(value: 'transform', child: Text('transform')),
                DropdownMenuItem(value: 'analysis', child: Text('analysis')),
                DropdownMenuItem(
                  value: 'ingest-connector',
                  child: Text('ingest-connector'),
                ),
              ],
              onChanged: (v) => setState(() => _kind = v ?? 'analysis'),
              decoration: const InputDecoration(labelText: 'Kind'),
            ),
            TextField(
              key: const ValueKey('space-main'),
              controller: _main,
              decoration: const InputDecoration(
                labelText: 'Main (existing code name)',
              ),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: _busy ? null : () => Navigator.pop(context, false),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey('space-create'),
          onPressed: _busy
              ? null
              : () async {
                  setState(() => _busy = true);
                  try {
                    await ref
                        .read(apiProvider)
                        .collectionCreate(
                          _name.text.trim(),
                          description: _about.text.trim(),
                          kind: _kind,
                          main: _main.text.trim(),
                        );
                    if (context.mounted) Navigator.pop(context, true);
                  } catch (e) {
                    if (context.mounted) _ok(context, '$e', color: m.error);
                  } finally {
                    if (mounted) setState(() => _busy = false);
                  }
                },
          child: const Text('Create'),
        ),
      ],
    );
  }
}

Future<void> _members(
  BuildContext context,
  WidgetRef ref,
  CollectionInfo c,
) async {
  await showDialog<void>(
    context: context,
    builder: (ctx) => _MembersDialog(space: c),
  );
  refreshProjectExtras(ref);
}

class _MembersDialog extends ConsumerStatefulWidget {
  const _MembersDialog({required this.space});
  final CollectionInfo space;
  @override
  ConsumerState<_MembersDialog> createState() => _MembersDialogState();
}

class _MembersDialogState extends ConsumerState<_MembersDialog> {
  late Future<List<SpaceMemberInfo>> _load;

  @override
  void initState() {
    super.initState();
    _reload();
  }

  void _reload() {
    _load = ref.read(apiProvider).collectionMembers(widget.space.id);
  }

  Future<void> _act(Future<String> call) async {
    final m = context.muninn;
    try {
      final msg = await call;
      if (mounted) _ok(context, msg);
      setState(_reload);
    } catch (e) {
      if (mounted) _ok(context, '$e', color: m.error);
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text(
        '${widget.space.name} · members',
        style: TextStyle(color: m.header),
      ),
      content: SizedBox(
        width: 480,
        child: FutureBuilder<List<SpaceMemberInfo>>(
          future: _load,
          builder: (context, snap) {
            if (snap.hasError) {
              return Text('${snap.error}', style: TextStyle(color: m.error));
            }
            final ms = snap.data;
            if (ms == null) return const LinearProgressIndicator();
            return Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text(
                  '★ is the main. Edits run at once and show as unreviewed. A pin freezes a member until you pin it again.',
                  style: TextStyle(color: m.muted),
                ),
                const SizedBox(height: 8),
                for (final mem in ms)
                  ListTile(
                    contentPadding: EdgeInsets.zero,
                    title: Text('${mem.main ? '★' : ' '} ${mem.alias}'),
                    subtitle: Text(
                      [
                        '${mem.codeName} (${mem.lang})',
                        mem.pinned ? 'pinned' : 'follows',
                        if (mem.pinBroken) 'changed since pinned',
                        if (mem.unreviewed) 'edited, unreviewed',
                      ].join(' · '),
                      style: TextStyle(
                        color: mem.pinBroken ? m.error : m.muted,
                      ),
                    ),
                    trailing: PopupMenuButton<String>(
                      onSelected: (v) {
                        final api = ref.read(apiProvider);
                        final id = widget.space.id;
                        switch (v) {
                          case 'main':
                            _act(api.collectionSetMain(id, mem.codeName));
                          case 'pin':
                            _act(
                              api.collectionPin(
                                id,
                                mem.codeName,
                                on: !mem.pinned,
                              ),
                            );
                          case 'drop':
                            _act(api.collectionRemoveCode(id, mem.codeName));
                        }
                      },
                      itemBuilder: (_) => [
                        if (!mem.main)
                          const PopupMenuItem(
                            value: 'main',
                            child: Text('Make main'),
                          ),
                        PopupMenuItem(
                          value: 'pin',
                          child: Text(mem.pinned ? 'Unpin' : 'Pin'),
                        ),
                        if (!mem.main)
                          const PopupMenuItem(
                            value: 'drop',
                            child: Text('Remove'),
                          ),
                      ],
                    ),
                  ),
                TextButton(
                  key: const ValueKey('space-add-member'),
                  onPressed: () async {
                    final code = await _ask(
                      context,
                      'Add code',
                      'Existing code name',
                    );
                    if (code == null || code.trim().isEmpty) return;
                    await _act(
                      ref
                          .read(apiProvider)
                          .collectionAdd(widget.space.id, code.trim()),
                    );
                  },
                  child: const Text('+ Add code…'),
                ),
              ],
            );
          },
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Close'),
        ),
      ],
    );
  }
}

Future<String?> _ask(BuildContext context, String title, String hint) {
  final c = TextEditingController();
  final m = context.muninn;
  return showDialog<String>(
    context: context,
    builder: (ctx) => AlertDialog(
      backgroundColor: m.bg,
      title: Text(title, style: TextStyle(color: m.header)),
      content: TextField(
        controller: c,
        decoration: InputDecoration(labelText: hint),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(ctx),
          child: const Text('Cancel'),
        ),
        FilledButton(
          onPressed: () => Navigator.pop(ctx, c.text),
          child: const Text('Add'),
        ),
      ],
    ),
  );
}

class _MigrateDialog extends ConsumerStatefulWidget {
  const _MigrateDialog();
  @override
  ConsumerState<_MigrateDialog> createState() => _MigrateDialogState();
}

class _MigrateDialogState extends ConsumerState<_MigrateDialog> {
  MigrateReport? _dry;
  String? _error;
  var _busy = false;
  var _kind = '';

  @override
  void initState() {
    super.initState();
    _preview();
  }

  Future<void> _preview() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final r = await ref
          .read(apiProvider)
          .migrateRun(
            dryRun: true,
            kind: _kind.trim().isEmpty ? null : _kind.trim(),
          );
      if (mounted) setState(() => _dry = r);
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    final r = _dry;
    final lines = <String>[
      if (r != null &&
          r.created.isEmpty &&
          r.skipped.isEmpty &&
          r.pipelines.isEmpty)
        'Nothing to migrate: every code is in a code space.',
      if (r != null)
        for (final c in r.created)
          '${c.renamed ? 'renamed' : 'created'}  ${c.renamed ? '${c.code} → ${c.space}' : c.space} (${c.kind})  ${c.why}',
      if (r != null)
        for (final s in r.skipped) 'skipped  ${s.code}  ${s.why}',
      if (r != null)
        for (final p in r.pipelines) 'pipeline $p: code: steps → space:',
    ];
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text('Migrate code to spaces', style: TextStyle(color: m.header)),
      content: SizedBox(
        width: 520,
        child: SingleChildScrollView(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                'Dry run. A backup is written first. Unclear kinds are skipped unless you set one below. Running it again changes nothing.',
                style: TextStyle(color: m.muted),
              ),
              const SizedBox(height: 8),
              TextField(
                key: const ValueKey('migrate-kind'),
                decoration: const InputDecoration(
                  labelText: 'Kind for unclear code (optional)',
                ),
                onChanged: (v) => _kind = v,
                onSubmitted: (_) => _preview(),
              ),
              Align(
                alignment: Alignment.centerLeft,
                child: TextButton(
                  onPressed: _busy ? null : _preview,
                  child: const Text('Refresh dry run'),
                ),
              ),
              if (_busy) const LinearProgressIndicator(),
              if (_error != null)
                Text(_error!, style: TextStyle(color: m.error)),
              const SizedBox(height: 8),
              Text(lines.join('\n'), style: TextStyle(color: m.fg)),
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey('migrate-go'),
          onPressed: _busy
              ? null
              : () async {
                  setState(() => _busy = true);
                  try {
                    final done = await ref
                        .read(apiProvider)
                        .migrateRun(
                          dryRun: false,
                          kind: _kind.trim().isEmpty ? null : _kind.trim(),
                          backup: true,
                        );
                    await ref.read(apiProvider).migrateDismiss();
                    if (context.mounted) Navigator.pop(context, done);
                  } catch (e) {
                    if (mounted) setState(() => _error = '$e');
                  } finally {
                    if (mounted) setState(() => _busy = false);
                  }
                },
          child: const Text('Migrate'),
        ),
      ],
    );
  }
}
