/// Create-then-share pickers (`mnnn share --create` / `--create-to`, TUI + New…).
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

/// Share [kind]/[item] to a target, optionally creating the item and/or destination.
Future<void> showCreateThenShareDialog(
  BuildContext context,
  WidgetRef ref, {
  String kind = 'journal',
  String item = '',
  String target = 'project',
}) async {
  if (!ref.read(unlockedProvider)) return;
  final result = await showDialog<ShareCreated>(
    context: context,
    builder: (ctx) => _CreateThenShareDialog(
      initialKind: kind,
      initialItem: item,
      initialTarget: target,
    ),
  );
  if (result != null && context.mounted) {
    _ok(context, result.summary.isEmpty ? 'shared' : result.summary);
    refreshProjectExtras(ref);
  }
}

class _CreateThenShareDialog extends ConsumerStatefulWidget {
  const _CreateThenShareDialog({
    required this.initialKind,
    required this.initialItem,
    required this.initialTarget,
  });
  final String initialKind, initialItem, initialTarget;

  @override
  ConsumerState<_CreateThenShareDialog> createState() =>
      _CreateThenShareDialogState();
}

class _CreateThenShareDialogState extends ConsumerState<_CreateThenShareDialog> {
  static const kinds = [
    'model',
    'search',
    'journal',
    'project',
    'report',
    'code',
    'space',
    'pipeline',
  ];
  static const targets = ['project', 'journal', 'code', 'space', 'net'];

  late String _kind;
  late String _target;
  late final TextEditingController _item;
  late final TextEditingController _to;
  late final TextEditingController _createTo;
  late final TextEditingController _note;
  var _createItem = false;
  var _createDest = false;
  var _busy = false;

  @override
  void initState() {
    super.initState();
    _kind = widget.initialKind;
    _target = widget.initialTarget;
    _item = TextEditingController(text: widget.initialItem);
    _to = TextEditingController();
    _createTo = TextEditingController();
    _note = TextEditingController();
    if (widget.initialItem.isEmpty) _createItem = true;
  }

  @override
  void dispose() {
    _item.dispose();
    _to.dispose();
    _createTo.dispose();
    _note.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text('Share', style: TextStyle(color: m.header)),
      content: SizedBox(
        width: 460,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text(
                'Create the item and/or destination (+ New…), then share — same as the TUI.',
                style: TextStyle(color: m.muted),
              ),
              const SizedBox(height: 8),
              DropdownButtonFormField<String>(
                key: const ValueKey('cts-kind'),
                initialValue: _kind,
                items: [
                  for (final k in kinds)
                    DropdownMenuItem(value: k, child: Text(k)),
                ],
                onChanged: (v) => setState(() => _kind = v ?? 'journal'),
                decoration: const InputDecoration(labelText: 'Item kind'),
              ),
              SwitchListTile(
                contentPadding: EdgeInsets.zero,
                title: const Text('+ New item (create then share)'),
                value: _createItem,
                onChanged: (v) => setState(() => _createItem = v),
              ),
              TextField(
                key: const ValueKey('cts-item'),
                controller: _item,
                decoration: InputDecoration(
                  labelText: _createItem ? 'New item name' : 'Existing name or id',
                ),
              ),
              DropdownButtonFormField<String>(
                key: const ValueKey('cts-target'),
                initialValue: _target,
                items: [
                  for (final k in targets)
                    DropdownMenuItem(value: k, child: Text(k)),
                ],
                onChanged: (v) => setState(() => _target = v ?? 'project'),
                decoration: const InputDecoration(labelText: 'Share to'),
              ),
              SwitchListTile(
                contentPadding: EdgeInsets.zero,
                title: Text(
                  _target == 'net'
                      ? '+ New exchange…'
                      : '+ New $_target…',
                ),
                value: _createDest,
                onChanged: (v) => setState(() => _createDest = v),
              ),
              if (_createDest)
                TextField(
                  key: const ValueKey('cts-create-to'),
                  controller: _createTo,
                  decoration: InputDecoration(
                    labelText: _target == 'net'
                        ? 'New exchange label'
                        : 'New $_target name',
                  ),
                )
              else if (_target != 'net')
                TextField(
                  key: const ValueKey('cts-to'),
                  controller: _to,
                  decoration: const InputDecoration(
                    labelText: 'Existing destination (name or id)',
                  ),
                ),
              if (_target == 'journal')
                TextField(
                  controller: _note,
                  decoration: const InputDecoration(labelText: 'Note'),
                ),
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: _busy ? null : () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey('cts-go'),
          onPressed: _busy
              ? null
              : () async {
                  setState(() => _busy = true);
                  try {
                    final created = await ref.read(apiProvider).shareCreateThen(
                          kind: _kind,
                          item: _item.text.trim(),
                          target: _target,
                          to: _createDest || _to.text.trim().isEmpty
                              ? const []
                              : [_to.text.trim()],
                          note: _note.text.trim(),
                          createItem: _createItem,
                          createTo: _createDest
                              ? _createTo.text.trim()
                              : null,
                        );
                    if (context.mounted) Navigator.pop(context, created);
                  } catch (e) {
                    if (context.mounted) _ok(context, '$e', color: m.error);
                  } finally {
                    if (mounted) setState(() => _busy = false);
                  }
                },
          child: const Text('Share'),
        ),
      ],
    );
  }
}

/// Review request with optional create of every ReviewKind.
Future<void> showCreateReviewSubjectDialog(
  BuildContext context,
  WidgetRef ref,
) async {
  if (!ref.read(unlockedProvider)) return;
  final m = context.muninn;
  final kinds = ['entry', 'journal', 'project', 'search', 'record'];
  var kind = 'journal';
  final name = TextEditingController();
  final journal = TextEditingController();
  final model = TextEditingController();
  final body = TextEditingController();
  final created = await showDialog<ShareCreated>(
    context: context,
    builder: (ctx) => StatefulBuilder(
      builder: (ctx, setLocal) => AlertDialog(
        backgroundColor: m.bg,
        title: Text('+ New review subject', style: TextStyle(color: m.header)),
        content: SizedBox(
          width: 420,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              DropdownButtonFormField<String>(
                initialValue: kind,
                items: [
                  for (final k in kinds)
                    DropdownMenuItem(value: k, child: Text(k)),
                ],
                onChanged: (v) => setLocal(() => kind = v ?? 'journal'),
                decoration: const InputDecoration(labelText: 'Kind'),
              ),
              TextField(
                controller: name,
                decoration: const InputDecoration(labelText: 'Name'),
              ),
              if (kind == 'entry')
                TextField(
                  controller: journal,
                  decoration: const InputDecoration(
                    labelText: 'Journal (optional → {name}-journal)',
                  ),
                ),
              if (kind == 'record')
                TextField(
                  controller: model,
                  decoration: const InputDecoration(labelText: 'Model'),
                ),
              if (kind == 'entry' || kind == 'record')
                TextField(
                  controller: body,
                  decoration: const InputDecoration(labelText: 'Body / fields'),
                ),
            ],
          ),
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx), child: const Text('Cancel')),
          FilledButton(
            onPressed: () async {
              try {
                final c = await ref.read(apiProvider).shareCreateReviewSubject(
                      kind,
                      name.text.trim(),
                      journal: journal.text.trim().isEmpty
                          ? null
                          : journal.text.trim(),
                      model:
                          model.text.trim().isEmpty ? null : model.text.trim(),
                      body: body.text.trim().isEmpty ? null : body.text.trim(),
                    );
                if (ctx.mounted) Navigator.pop(ctx, c);
              } catch (e) {
                if (ctx.mounted) _ok(ctx, '$e', color: m.error);
              }
            },
            child: const Text('Create'),
          ),
        ],
      ),
    ),
  );
  name.dispose();
  journal.dispose();
  model.dispose();
  body.dispose();
  if (created != null && context.mounted) {
    _ok(context, created.summary);
    refreshProjectExtras(ref);
  }
}
