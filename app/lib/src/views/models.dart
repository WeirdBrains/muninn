/// Model designer (TUI Data ▸ Model designer… / `D` / `mnnn model`).
/// Create and edit fields as a draft, restore dropped fields by id, import
/// and export templates, create/edit records, preview and apply migrations,
/// roll versions back, and attach, dry-run or run model-owned rules.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../state.dart';
import '../theme/muninn_theme.dart';

Future<void> openModelsPane(
  WidgetRef ref, {
  String? modelId,
  String? recordId,
}) async {
  ref.read(sectionProvider.notifier).state = Section.projects;
  ref.read(projectPaneProvider.notifier).state = ProjectPane.models;
  ref.read(viewingModelIdProvider.notifier).state = modelId;
  ref.read(viewingRecordIdProvider.notifier).state = recordId;
}

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
          key: const ValueKey('model-confirm'),
          onPressed: () => Navigator.pop(ctx, true),
          child: const Text('OK'),
        ),
      ],
    ),
  );
  return yes == true;
}

Future<String?> _prompt(
  BuildContext context, {
  required String title,
  required String label,
  String value = '',
  String keyName = 'model-prompt',
  int maxLines = 1,
}) {
  return showDialog<String>(
    context: context,
    builder: (ctx) => _PromptDialog(
      title: title,
      label: label,
      value: value,
      keyName: keyName,
      maxLines: maxLines,
    ),
  );
}

class _PromptDialog extends StatefulWidget {
  const _PromptDialog({
    required this.title,
    required this.label,
    required this.value,
    required this.keyName,
    required this.maxLines,
  });
  final String title, label, value, keyName;
  final int maxLines;
  @override
  State<_PromptDialog> createState() => _PromptDialogState();
}

class _PromptDialogState extends State<_PromptDialog> {
  late final TextEditingController _c;

  @override
  void initState() {
    super.initState();
    _c = TextEditingController(text: widget.value);
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text(widget.title, style: TextStyle(color: m.header)),
      content: TextField(
        key: ValueKey(widget.keyName),
        controller: _c,
        autofocus: true,
        maxLines: widget.maxLines,
        decoration: InputDecoration(labelText: widget.label),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: ValueKey('${widget.keyName}-save'),
          onPressed: () => Navigator.pop(context, _c.text),
          child: const Text('Save'),
        ),
      ],
    );
  }
}

Future<void> _showLines(
  BuildContext context,
  String title,
  List<String> lines,
) async {
  if (!context.mounted) return;
  final m = context.muninn;
  await showDialog<void>(
    context: context,
    builder: (ctx) => AlertDialog(
      backgroundColor: m.bg,
      title: Text(title, style: TextStyle(color: m.header)),
      content: SizedBox(
        width: 520,
        height: 360,
        child: SelectableText(
          lines.isEmpty ? '(no output)' : lines.join('\n'),
          style: TextStyle(color: m.fg, fontFamily: 'monospace'),
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

Future<void> _exportTemplate(
  BuildContext context,
  WidgetRef ref,
  ModelInfo model,
) async {
  final path = await _prompt(
    context,
    title: 'Export template',
    label: 'Path (.template.json)',
    value: '${model.name}.template.json',
    keyName: 'model-export-path',
  );
  if (path == null || path.trim().isEmpty) return;
  try {
    final msg = await ref
        .read(apiProvider)
        .modelExportTemplate(model.id, path.trim());
    refreshProjectExtras(ref);
    if (context.mounted) _ok(context, msg);
  } catch (e) {
    if (context.mounted) _ok(context, '$e', color: context.muninn.error);
  }
}

Future<void> _importTemplate(BuildContext context, WidgetRef ref) async {
  final source = await _prompt(
    context,
    title: 'Import template',
    label: 'Path or builtin:bars',
    value: 'builtin:bars',
    keyName: 'model-import-source',
  );
  if (source == null || source.trim().isEmpty) return;
  try {
    final api = ref.read(apiProvider);
    var name = '';
    var merge = false;
    var preview = await api.modelImportPreview(source.trim());
    if (preview.clash) {
      if (!context.mounted) return;
      final picked = await _prompt(
        context,
        title: 'Model ${preview.model} exists',
        label: 'A new name, or merge',
        value: preview.suggestedName,
        keyName: 'model-import-name',
      );
      if (picked == null) return;
      if (picked.trim().toLowerCase() == 'merge') {
        merge = true;
      } else {
        name = picked.trim();
      }
      preview = await api.modelImportPreview(
        source.trim(),
        name: name.isEmpty ? null : name,
        merge: merge,
      );
    }
    if (preview.blocked.isNotEmpty) {
      if (context.mounted) {
        _ok(context, preview.blocked.join('; '), color: context.muninn.error);
      }
      return;
    }
    var destructive = false;
    if (preview.destructive) {
      if (!context.mounted) return;
      if (!await _confirm(
        context,
        'Import template',
        'This loses ${preview.lost} value${preview.lost == 1 ? '' : 's'}. Import anyway?',
      )) {
        return;
      }
      destructive = true;
    }
    final msg = await api.modelImportTemplate(
      source.trim(),
      name: name.isEmpty ? null : name,
      merge: merge,
      destructive: destructive,
    );
    refreshProjectExtras(ref);
    if (context.mounted) _ok(context, msg);
  } catch (e) {
    if (context.mounted) _ok(context, '$e', color: context.muninn.error);
  }
}

class ModelsPane extends ConsumerWidget {
  const ModelsPane({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final modelId = ref.watch(viewingModelIdProvider);
    final recordId = ref.watch(viewingRecordIdProvider);
    return ref
        .watch(modelsProvider)
        .when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Padding(
            padding: const EdgeInsets.all(16),
            child: Text('$e', style: TextStyle(color: m.error)),
          ),
          data: (items) {
            final all = items ?? const <ModelInfo>[];
            ModelInfo? selected;
            for (final x in all) {
              if (x.id == modelId ||
                  x.name == modelId ||
                  x.shortId == modelId) {
                selected = x;
                break;
              }
            }
            return Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                SizedBox(
                  width: 280,
                  child: ListView(
                    padding: const EdgeInsets.all(8),
                    children: [
                      Text('Models', style: TextStyle(color: m.muted)),
                      const SizedBox(height: 8),
                      FilledButton.icon(
                        key: const ValueKey('model-new'),
                        onPressed: () => _newModel(context, ref),
                        icon: const Icon(Icons.add),
                        label: const Text('New model…'),
                      ),
                      const SizedBox(height: 8),
                      OutlinedButton.icon(
                        key: const ValueKey('model-import'),
                        onPressed: () => _importTemplate(context, ref),
                        icon: const Icon(Icons.file_download_outlined),
                        label: const Text('Import template…'),
                      ),
                      const SizedBox(height: 8),
                      if (all.isEmpty)
                        Padding(
                          padding: const EdgeInsets.only(top: 16),
                          child: Text(
                            'No models yet. Create one here, or in the TUI '
                            '(Data ▸ Model designer…) / `mnnn model`.',
                            style: TextStyle(color: m.muted),
                          ),
                        ),
                      for (final model in all)
                        ListTile(
                          key: ValueKey('model-${model.id}'),
                          selected: selected?.id == model.id,
                          title: Text(model.name),
                          subtitle: Text(
                            [
                              if (model.description.isNotEmpty)
                                model.description,
                              if (model.status.isNotEmpty) model.status,
                              '${model.recordCount} records',
                              if (model.hasDraft) 'draft',
                              if (model.fields.isNotEmpty)
                                model.fields.join(', '),
                            ].join(' · '),
                            style: TextStyle(color: m.muted),
                          ),
                          onTap: () {
                            ref.read(viewingModelIdProvider.notifier).state =
                                model.id;
                            ref.read(viewingRecordIdProvider.notifier).state =
                                null;
                            ref.read(modelRecordsQueryProvider.notifier).state =
                                '';
                          },
                        ),
                    ],
                  ),
                ),
                VerticalDivider(width: 1, color: m.border),
                Expanded(
                  child: selected == null
                      ? Center(
                          child: Text(
                            'Pick a model',
                            style: TextStyle(color: m.muted),
                          ),
                        )
                      : _ModelWorkspace(model: selected, recordId: recordId),
                ),
              ],
            );
          },
        );
  }
}

Future<void> _newModel(BuildContext context, WidgetRef ref) async {
  final created = await showDialog<bool>(
    context: context,
    builder: (ctx) => const _NewModelDialog(),
  );
  if (created == true) refreshProjectExtras(ref);
}

class _NewModelDialog extends ConsumerStatefulWidget {
  const _NewModelDialog();
  @override
  ConsumerState<_NewModelDialog> createState() => _NewModelDialogState();
}

class _NewModelDialogState extends ConsumerState<_NewModelDialog> {
  final _name = TextEditingController();
  final _desc = TextEditingController();
  final _fields = TextEditingController();
  var _draft = true;

  @override
  void dispose() {
    _name.dispose();
    _desc.dispose();
    _fields.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    try {
      final m = await ref
          .read(apiProvider)
          .modelCreate(
            _name.text.trim(),
            description: _desc.text.trim(),
            fields: _fields.text.trim(),
            draft: _draft,
          );
      ref.read(viewingModelIdProvider.notifier).state = m.id;
      if (mounted) Navigator.pop(context, true);
    } catch (e) {
      if (mounted) _ok(context, '$e', color: context.muninn.error);
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text('New model', style: TextStyle(color: m.header)),
      content: SizedBox(
        width: 520,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              key: const ValueKey('model-name'),
              controller: _name,
              autofocus: true,
              decoration: const InputDecoration(labelText: 'Name'),
            ),
            TextField(
              key: const ValueKey('model-about'),
              controller: _desc,
              decoration: const InputDecoration(labelText: 'Description'),
            ),
            TextField(
              key: const ValueKey('model-fields'),
              controller: _fields,
              maxLines: 3,
              decoration: const InputDecoration(
                labelText:
                    'Fields (name:type [required] [unique] [default=V]; …)',
                hintText: 'symbol:text!; price:real',
              ),
            ),
            CheckboxListTile(
              contentPadding: EdgeInsets.zero,
              title: const Text('Save as draft (A applies it as v1)'),
              value: _draft,
              onChanged: (v) => setState(() => _draft = v ?? true),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context, false),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey('model-save'),
          onPressed: _save,
          child: const Text('Create'),
        ),
      ],
    );
  }
}

class _ModelWorkspace extends ConsumerStatefulWidget {
  const _ModelWorkspace({required this.model, this.recordId});
  final ModelInfo model;
  final String? recordId;
  @override
  ConsumerState<_ModelWorkspace> createState() => _ModelWorkspaceState();
}

class _ModelWorkspaceState extends ConsumerState<_ModelWorkspace>
    with SingleTickerProviderStateMixin {
  late final TabController _tabs;

  @override
  void initState() {
    super.initState();
    _tabs = TabController(
      length: 4,
      vsync: this,
      initialIndex: widget.recordId != null ? 1 : 0,
    );
  }

  @override
  void didUpdateWidget(covariant _ModelWorkspace oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.recordId != null && widget.recordId != oldWidget.recordId) {
      _tabs.index = 1;
    }
  }

  @override
  void dispose() {
    _tabs.dispose();
    super.dispose();
  }

  Future<void> _run(Future<void> Function() fn) async {
    try {
      await fn();
      refreshProjectExtras(ref);
    } catch (e) {
      if (mounted) _ok(context, '$e', color: context.muninn.error);
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    final model = widget.model;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 12, 16, 0),
          child: Row(
            children: [
              Expanded(
                child: Text(
                  [
                    model.name,
                    if (model.status.isNotEmpty) model.status,
                    if (model.hasDraft) 'draft',
                  ].join(' · '),
                  style: Theme.of(context).textTheme.titleMedium,
                ),
              ),
              TextButton(
                onPressed: () async {
                  final v = await _prompt(
                    context,
                    title: 'Model name',
                    label: 'Name',
                    value: model.name,
                    keyName: 'model-rename',
                  );
                  if (v == null) return;
                  await _run(
                    () async =>
                        ref.read(apiProvider).modelRename(model.id, name: v),
                  );
                },
                child: const Text('Rename…'),
              ),
              TextButton(
                onPressed: () async {
                  final v = await _prompt(
                    context,
                    title: 'Description',
                    label: 'Description',
                    value: model.description,
                    keyName: 'model-desc',
                    maxLines: 3,
                  );
                  if (v == null) return;
                  await _run(
                    () async => ref
                        .read(apiProvider)
                        .modelRename(model.id, description: v),
                  );
                },
                child: const Text('Description…'),
              ),
              TextButton(
                onPressed: () async {
                  final warn = model.isBars
                      ? 'Remove ${model.name}? This deletes all ${model.recordCount} bars and the fields you added; it starts again as shipped.'
                      : 'Remove model ${model.name} and its ${model.recordCount} records? Its rules become standalone and off.';
                  if (!await _confirm(context, 'Remove model', warn)) return;
                  await _run(() async {
                    final msg = await ref
                        .read(apiProvider)
                        .modelRemove(model.id, destructive: true);
                    ref.read(viewingModelIdProvider.notifier).state = null;
                    if (context.mounted) _ok(context, msg);
                  });
                },
                child: Text('Remove', style: TextStyle(color: m.error)),
              ),
            ],
          ),
        ),
        if (model.description.isNotEmpty)
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            child: Text(model.description, style: TextStyle(color: m.muted)),
          ),
        TabBar(
          controller: _tabs,
          labelColor: m.header,
          tabs: const [
            Tab(key: ValueKey('model-tab-design'), text: 'Design'),
            Tab(key: ValueKey('model-tab-records'), text: 'Records'),
            Tab(key: ValueKey('model-tab-history'), text: 'History'),
            Tab(key: ValueKey('model-tab-rules'), text: 'Rules'),
          ],
        ),
        Expanded(
          child: TabBarView(
            controller: _tabs,
            children: [
              _DesignTab(model: model, run: _run),
              _RecordsColumn(model: model, recordId: widget.recordId),
              _HistoryTab(model: model, run: _run),
              _RulesTab(model: model),
            ],
          ),
        ),
      ],
    );
  }
}

class _DesignTab extends ConsumerWidget {
  const _DesignTab({required this.model, required this.run});
  final ModelInfo model;
  final Future<void> Function(Future<void> Function()) run;

  Future<void> _apply(BuildContext context, WidgetRef ref) async {
    try {
      final plan = await ref.read(apiProvider).modelPlan(model.id);
      if (plan == null) {
        if (context.mounted) {
          _ok(context, '${model.name} has no changes to apply');
        }
        return;
      }
      if (plan.blocked.isNotEmpty) {
        if (context.mounted) {
          _ok(context, plan.blocked.join('; '), color: context.muninn.error);
        }
        return;
      }
      var destructive = false;
      if (plan.destructive) {
        if (!context.mounted) return;
        if (!await _confirm(
          context,
          'Apply migration',
          'This loses ${plan.lost} value${plan.lost == 1 ? '' : 's'}. Apply anyway?',
        )) {
          return;
        }
        destructive = true;
      }
      final done = await ref
          .read(apiProvider)
          .modelApply(model.id, destructive: destructive);
      refreshProjectExtras(ref);
      if (context.mounted) {
        _ok(context, 'applied ${model.name} v${done.from} → v${done.to}');
      }
    } catch (e) {
      if (context.mounted) _ok(context, '$e', color: context.muninn.error);
    }
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final fields = model.designFields;
    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            FilledButton.icon(
              key: const ValueKey('model-add-field'),
              onPressed: () async {
                final v = await _prompt(
                  context,
                  title: 'New field',
                  label: 'name:type [required] [unique] [index] [default=V] [choices=a|b]',
                  keyName: 'field-spec',
                );
                if (v == null || v.trim().isEmpty) return;
                await run(
                  () async => ref
                      .read(apiProvider)
                      .modelEdit(model.id, add: [v.trim()]),
                );
              },
              icon: const Icon(Icons.add),
              label: const Text('Add field…'),
            ),
            FilledButton(
              key: const ValueKey('model-apply'),
              onPressed: model.hasDraft || model.version == 0
                  ? () => _apply(context, ref)
                  : null,
              child: const Text('Apply'),
            ),
            OutlinedButton(
              key: const ValueKey('model-discard'),
              onPressed: model.hasDraft
                  ? () => run(() async {
                      await ref.read(apiProvider).modelDiscard(model.id);
                    })
                  : null,
              child: const Text('Discard draft'),
            ),
            OutlinedButton.icon(
              key: const ValueKey('model-export'),
              onPressed: () => _exportTemplate(context, ref, model),
              icon: const Icon(Icons.file_upload_outlined),
              label: const Text('Export template…'),
            ),
          ],
        ),
        const SizedBox(height: 12),
        if (fields.isEmpty)
          Text(
            'No fields yet. Add one, then Apply.',
            style: TextStyle(color: m.muted),
          ),
        for (final i in Iterable<int>.generate(fields.length))
          ListTile(
            key: ValueKey('design-field-${fields[i].id}'),
            contentPadding: EdgeInsets.zero,
            leading: Text(
              fields[i].mark.trim().isEmpty ? '·' : fields[i].mark,
              style: TextStyle(
                color: fields[i].dropped
                    ? m.error
                    : fields[i].mark.trim() == '+'
                    ? m.header
                    : m.fg,
                fontFamily: 'monospace',
              ),
            ),
            title: Text(
              fields[i].spec.isEmpty ? fields[i].name : fields[i].spec,
            ),
            subtitle: Text(
              [
                fields[i].ty,
                if (fields[i].required) 'required',
                if (fields[i].unique) 'unique',
                if (fields[i].indexed) 'index',
                if (fields[i].builtin) 'from imports',
                if (fields[i].dropped) 'dropped in draft',
                if (fields[i].about.isNotEmpty) fields[i].about,
              ].join(' · '),
              style: TextStyle(color: m.muted),
            ),
            trailing: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                IconButton(
                  tooltip: 'Move up',
                  onPressed: fields[i].dropped || i == 0
                      ? null
                      : () => run(
                          () async => ref
                              .read(apiProvider)
                              .modelEdit(
                                model.id,
                                moves: ['${fields[i].name}:$i'],
                              ),
                        ),
                  icon: const Icon(Icons.arrow_upward),
                ),
                IconButton(
                  tooltip: 'Move down',
                  onPressed: fields[i].dropped || i == fields.length - 1
                      ? null
                      : () => run(
                          () async => ref
                              .read(apiProvider)
                              .modelEdit(
                                model.id,
                                moves: ['${fields[i].name}:${i + 2}'],
                              ),
                        ),
                  icon: const Icon(Icons.arrow_downward),
                ),
                IconButton(
                  tooltip: 'Edit',
                  onPressed: fields[i].dropped
                      ? null
                      : () async {
                          final v = await _prompt(
                            context,
                            title: 'Field',
                            label: 'name:type and constraints; renaming keeps values',
                            value: fields[i].spec,
                            keyName: 'field-edit',
                          );
                          if (v == null) return;
                          await run(
                            () async => ref
                                .read(apiProvider)
                                .modelSetField(
                                  model.id,
                                  fields[i].id,
                                  v.trim(),
                                ),
                          );
                        },
                  icon: const Icon(Icons.edit_outlined),
                ),
                IconButton(
                  key: ValueKey('design-field-action-${fields[i].id}'),
                  tooltip: fields[i].dropped
                      ? 'Restore (same field id)'
                      : fields[i].mark.trim() == '~'
                      ? 'Undo change'
                      : 'Drop',
                  onPressed: () => run(() async {
                    if (fields[i].dropped || fields[i].mark.trim() == '~') {
                      await ref
                          .read(apiProvider)
                          .modelRestoreField(model.id, fields[i].id);
                    } else {
                      await ref
                          .read(apiProvider)
                          .modelEdit(model.id, drop: [fields[i].name]);
                    }
                  }),
                  icon: Icon(
                    fields[i].dropped || fields[i].mark.trim() == '~'
                        ? Icons.undo
                        : Icons.delete_outline,
                  ),
                ),
              ],
            ),
          ),
        const SizedBox(height: 16),
        Text('Migration plan', style: TextStyle(color: m.muted)),
        const SizedBox(height: 8),
        _PlanView(modelId: model.id),
      ],
    );
  }
}

class _PlanView extends ConsumerWidget {
  const _PlanView({required this.modelId});
  final String modelId;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return ref
        .watch(modelPlanProvider)
        .when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Text('$e', style: TextStyle(color: m.error)),
          data: (plan) {
            if (plan == null) {
              return Text(
                'No pending migration.',
                style: TextStyle(color: m.muted),
              );
            }
            return Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'v${plan.from} → v${plan.to} · ${plan.rows} records'
                  '${plan.destructive ? ' · loses ${plan.lost} values' : ''}',
                ),
                if (plan.blocked.isNotEmpty)
                  Padding(
                    padding: const EdgeInsets.only(top: 8),
                    child: Text(
                      plan.blocked.join('\n'),
                      style: TextStyle(color: m.error),
                    ),
                  ),
                for (final line in plan.lines)
                  Text(line, style: const TextStyle(fontFamily: 'monospace')),
              ],
            );
          },
        );
  }
}

class _HistoryTab extends ConsumerWidget {
  const _HistoryTab({required this.model, required this.run});
  final ModelInfo model;
  final Future<void> Function(Future<void> Function()) run;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return ref
        .watch(modelHistoryProvider)
        .when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Padding(
            padding: const EdgeInsets.all(16),
            child: Text('$e', style: TextStyle(color: m.error)),
          ),
          data: (items) {
            final all = items ?? const <ModelVersionInfo>[];
            if (all.isEmpty) {
              return Padding(
                padding: const EdgeInsets.all(16),
                child: Text(
                  '${model.name} has no applied versions yet.',
                  style: TextStyle(color: m.muted),
                ),
              );
            }
            return ListView(
              padding: const EdgeInsets.all(16),
              children: [
                for (final v in all)
                  ListTile(
                    key: ValueKey('model-version-${v.version}'),
                    contentPadding: EdgeInsets.zero,
                    title: Text(
                      'v${v.version}${v.current ? ' *' : ''} · ${v.fieldCount} fields',
                    ),
                    subtitle: Text(
                      [
                        if (v.note.isNotEmpty) v.note,
                        if (v.author.isNotEmpty) v.author,
                        ...v.steps,
                      ].join('\n'),
                      style: TextStyle(color: m.muted),
                    ),
                    isThreeLine: v.steps.isNotEmpty,
                    trailing: v.current
                        ? null
                        : TextButton(
                            onPressed: () async {
                              if (!await _confirm(
                                context,
                                'Roll back',
                                'Roll ${model.name} back to the design of v${v.version} as the next version?',
                              )) {
                                return;
                              }
                              await run(() async {
                                final plan = await ref
                                    .read(apiProvider)
                                    .modelRollbackPlan(model.id, v.version);
                                var destructive = plan.destructive;
                                if (destructive && context.mounted) {
                                  if (!await _confirm(
                                    context,
                                    'Roll back',
                                    'This loses ${plan.lost} values. Continue?',
                                  )) {
                                    return;
                                  }
                                }
                                final done = await ref
                                    .read(apiProvider)
                                    .modelRollback(
                                      model.id,
                                      v.version,
                                      destructive: destructive,
                                    );
                                if (context.mounted) {
                                  _ok(
                                    context,
                                    '${model.name} is now v${done.to} (design of v${v.version})',
                                  );
                                }
                              });
                            },
                            child: const Text('Roll back'),
                          ),
                  ),
              ],
            );
          },
        );
  }
}

class _RulesTab extends ConsumerWidget {
  const _RulesTab({required this.model});
  final ModelInfo model;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return ref
        .watch(rulesProvider)
        .when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Padding(
            padding: const EdgeInsets.all(16),
            child: Text('$e', style: TextStyle(color: m.error)),
          ),
          data: (items) {
            final all = [
              for (final r in items ?? const <RuleInfo>[])
                if (r.home == 'model ${model.name}' ||
                    r.home.startsWith('model ${model.name},'))
                  r,
            ];
            return ListView(
              padding: const EdgeInsets.all(16),
              children: [
                FilledButton.icon(
                  key: const ValueKey('model-rule-new'),
                  onPressed: () async {
                    final created = await showDialog<bool>(
                      context: context,
                      builder: (ctx) => _ModelRuleDialog(model: model),
                    );
                    if (created == true) refreshProjectExtras(ref);
                  },
                  icon: const Icon(Icons.add),
                  label: const Text('New rule…'),
                ),
                const SizedBox(height: 12),
                if (all.isEmpty)
                  Text(
                    'No rules feed this model yet. A rule on this model '
                    'runs when something happens (import, record.create, …).',
                    style: TextStyle(color: m.muted),
                  )
                else
                  for (final r in all)
                    ListTile(
                      key: ValueKey('model-rule-${r.id}'),
                      contentPadding: EdgeInsets.zero,
                      title: Text(r.name),
                      subtitle: Text(
                        [
                          r.enabled ? 'on' : 'off',
                          if (r.brief.isNotEmpty) r.brief,
                        ].join(' · '),
                        style: TextStyle(color: m.muted),
                      ),
                      trailing: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          TextButton(
                            key: ValueKey('model-rule-test-${r.id}'),
                            onPressed: () async {
                              try {
                                final lines = await ref
                                    .read(apiProvider)
                                    .ruleTest(rule: r.id);
                                if (context.mounted) {
                                  await _showLines(
                                    context,
                                    'Test ${r.name}',
                                    lines,
                                  );
                                }
                              } catch (e) {
                                if (context.mounted) {
                                  _ok(context, '$e', color: m.error);
                                }
                              }
                            },
                            child: const Text('Test'),
                          ),
                          TextButton(
                            key: ValueKey('model-rule-dry-${r.id}'),
                            onPressed: () async {
                              try {
                                final lines = await ref
                                    .read(apiProvider)
                                    .ruleDryRun(rule: r.id);
                                if (context.mounted) {
                                  await _showLines(
                                    context,
                                    'Dry run ${r.name}',
                                    lines,
                                  );
                                }
                              } catch (e) {
                                if (context.mounted) {
                                  _ok(context, '$e', color: m.error);
                                }
                              }
                            },
                            child: const Text('Dry run'),
                          ),
                          TextButton(
                            key: ValueKey('model-rule-try-${r.id}'),
                            onPressed: () async {
                              try {
                                final lines = await ref
                                    .read(apiProvider)
                                    .modelRuleTry(r.id);
                                refreshProjectExtras(ref);
                                if (context.mounted) {
                                  await _showLines(
                                    context,
                                    'Run ${r.name}',
                                    lines,
                                  );
                                }
                              } catch (e) {
                                if (context.mounted) {
                                  _ok(context, '$e', color: m.error);
                                }
                              }
                            },
                            child: const Text('Run'),
                          ),
                          TextButton(
                            onPressed: () async {
                              try {
                                if (r.enabled) {
                                  await ref.read(apiProvider).ruleDisable(r.id);
                                } else {
                                  await ref.read(apiProvider).ruleEnable(r.id);
                                }
                                refreshProjectExtras(ref);
                              } catch (e) {
                                if (context.mounted) {
                                  _ok(context, '$e', color: m.error);
                                }
                              }
                            },
                            child: Text(r.enabled ? 'Disable' : 'Enable'),
                          ),
                          TextButton(
                            onPressed: () async {
                              if (!await _confirm(
                                context,
                                'Delete rule',
                                'Delete rule ${r.name}?',
                              )) {
                                return;
                              }
                              try {
                                await ref.read(apiProvider).ruleRemove(r.id);
                                refreshProjectExtras(ref);
                              } catch (e) {
                                if (context.mounted) {
                                  _ok(context, '$e', color: m.error);
                                }
                              }
                            },
                            child: Text(
                              'Remove',
                              style: TextStyle(color: m.error),
                            ),
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

class _ModelRuleDialog extends ConsumerStatefulWidget {
  const _ModelRuleDialog({required this.model});
  final ModelInfo model;
  @override
  ConsumerState<_ModelRuleDialog> createState() => _ModelRuleDialogState();
}

class _ModelRuleDialogState extends ConsumerState<_ModelRuleDialog> {
  final _name = TextEditingController();
  final _on = TextEditingController(text: 'record.create');
  final _when = TextEditingController();
  final _then = TextEditingController();

  @override
  void dispose() {
    _name.dispose();
    _on.dispose();
    _when.dispose();
    _then.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    try {
      await ref
          .read(apiProvider)
          .ruleCreate(
            _name.text.trim(),
            on: _on.text.trim(),
            when: _when.text.trim(),
            then: _then.text.trim(),
            home: 'model:${widget.model.name}',
          );
      if (mounted) Navigator.pop(context, true);
    } catch (e) {
      if (mounted) _ok(context, '$e', color: context.muninn.error);
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text(
        'New rule on ${widget.model.name}',
        style: TextStyle(color: m.header),
      ),
      content: SizedBox(
        width: 520,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              key: const ValueKey('model-rule-name'),
              controller: _name,
              decoration: const InputDecoration(labelText: 'Name'),
            ),
            TextField(
              key: const ValueKey('model-rule-on'),
              controller: _on,
              decoration: const InputDecoration(
                labelText: 'On (import, journal.*, record.create, *)',
              ),
            ),
            TextField(
              key: const ValueKey('model-rule-if'),
              controller: _when,
              decoration: const InputDecoration(
                labelText: 'If (FIELD OP VALUE; …, empty: always)',
              ),
            ),
            TextField(
              key: const ValueKey('model-rule-then'),
              controller: _then,
              maxLines: 3,
              decoration: const InputDecoration(
                labelText: 'Then (record-create …; status …)',
              ),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context, false),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey('model-rule-save'),
          onPressed: _save,
          child: const Text('Create'),
        ),
      ],
    );
  }
}

class _RecordsColumn extends ConsumerWidget {
  const _RecordsColumn({required this.model, this.recordId});
  final ModelInfo model;
  final String? recordId;

  String _assignments(RecordInfo r) => [
    for (final f in r.fields)
      if (!f.name.startsWith('_') && f.name != 'id') '${f.name}=${f.value}',
  ].join('; ');

  Future<void> _newRecord(BuildContext context, WidgetRef ref) async {
    final values = await _prompt(
      context,
      title: 'New record',
      label: 'field=value; field=value',
      keyName: 'record-values',
    );
    if (values == null || values.trim().isEmpty) return;
    try {
      final rec = await ref
          .read(apiProvider)
          .modelRecordCreate(model.id, values.trim());
      refreshProjectExtras(ref);
      ref.read(viewingRecordIdProvider.notifier).state = rec.id;
      if (context.mounted) {
        _ok(context, 'created ${model.name} record ${rec.shortId}');
      }
    } catch (e) {
      if (context.mounted) _ok(context, '$e', color: context.muninn.error);
    }
  }

  Future<void> _bulkUpdate(BuildContext context, WidgetRef ref) async {
    final text = ref.read(modelRecordsQueryProvider).trim();
    if (text.isEmpty) {
      _ok(
        context,
        'set a filter first: bulk update changes every matching record',
      );
      return;
    }
    final values = await _prompt(
      context,
      title: 'Update matching records',
      label: 'field=value; field=value',
      keyName: 'record-bulk-values',
    );
    if (values == null || values.trim().isEmpty) return;
    if (!context.mounted) return;
    if (!await _confirm(
      context,
      'Update matching records',
      'Set ${values.trim()} on every ${model.name} record matching "$text"?',
    )) {
      return;
    }
    try {
      final msg = await ref
          .read(apiProvider)
          .modelRecordUpdateMany(model.id, text, values.trim());
      refreshProjectExtras(ref);
      if (context.mounted) _ok(context, msg);
    } catch (e) {
      if (context.mounted) _ok(context, '$e', color: context.muninn.error);
    }
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final query = ref.watch(modelRecordsQueryProvider);
    return ref
        .watch(modelRecordsProvider)
        .when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Padding(
            padding: const EdgeInsets.all(16),
            child: Text('$e', style: TextStyle(color: m.error)),
          ),
          data: (items) {
            final rows = items ?? const <RecordInfo>[];
            return Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                SizedBox(
                  width: 280,
                  child: ListView(
                    padding: const EdgeInsets.all(8),
                    children: [
                      Text(
                        '${model.name} · ${rows.length} shown',
                        style: TextStyle(color: m.muted),
                      ),
                      const SizedBox(height: 8),
                      TextField(
                        key: const ValueKey('model-record-filter'),
                        decoration: InputDecoration(
                          labelText: query.isEmpty
                              ? 'Filter'
                              : 'Filter · $query',
                          isDense: true,
                        ),
                        onSubmitted: (v) =>
                            ref.read(modelRecordsQueryProvider.notifier).state =
                                v,
                      ),
                      const SizedBox(height: 8),
                      FilledButton.icon(
                        key: const ValueKey('model-record-new'),
                        onPressed: () => _newRecord(context, ref),
                        icon: const Icon(Icons.add),
                        label: const Text('New record…'),
                      ),
                      const SizedBox(height: 8),
                      OutlinedButton.icon(
                        key: const ValueKey('model-record-bulk'),
                        onPressed: () => _bulkUpdate(context, ref),
                        icon: const Icon(Icons.edit_note),
                        label: const Text('Update matching…'),
                      ),
                      if (rows.isEmpty)
                        Padding(
                          padding: const EdgeInsets.only(top: 16),
                          child: Text(
                            'No records',
                            style: TextStyle(color: m.muted),
                          ),
                        ),
                      for (final r in rows)
                        ListTile(
                          key: ValueKey('record-${r.id}'),
                          selected: r.id == recordId,
                          title: Text(_brief(r)),
                          subtitle: Text(
                            r.shortId,
                            style: TextStyle(color: m.muted),
                          ),
                          onTap: () =>
                              ref.read(viewingRecordIdProvider.notifier).state =
                                  r.id,
                        ),
                    ],
                  ),
                ),
                VerticalDivider(width: 1, color: m.border),
                Expanded(
                  child: _RecordDetail(
                    model: model,
                    recordId: recordId,
                    assignmentsOf: _assignments,
                  ),
                ),
              ],
            );
          },
        );
  }

  String _brief(RecordInfo r) {
    for (final f in r.fields) {
      if (f.name.startsWith('_') || f.name == 'id') continue;
      if (f.value.isNotEmpty) return f.value;
    }
    return r.shortId;
  }
}

class _RecordDetail extends ConsumerWidget {
  const _RecordDetail({
    required this.model,
    this.recordId,
    required this.assignmentsOf,
  });
  final ModelInfo model;
  final String? recordId;
  final String Function(RecordInfo) assignmentsOf;

  Future<void> _edit(
    BuildContext context,
    WidgetRef ref,
    RecordInfo rec,
  ) async {
    final values = await _prompt(
      context,
      title: 'Edit record',
      label: 'field=value; an empty value clears it',
      value: assignmentsOf(rec),
      keyName: 'record-edit-values',
      maxLines: 4,
    );
    if (values == null) return;
    try {
      await ref.read(apiProvider).modelRecordUpdate(model.id, rec.id, values);
      refreshProjectExtras(ref);
      if (context.mounted) {
        _ok(context, 'updated ${model.name} record ${rec.shortId}');
      }
    } catch (e) {
      if (context.mounted) _ok(context, '$e', color: context.muninn.error);
    }
  }

  Future<void> _delete(
    BuildContext context,
    WidgetRef ref,
    RecordInfo rec,
  ) async {
    if (!await _confirm(
      context,
      'Delete record',
      'Delete ${model.name} record ${rec.shortId}?',
    )) {
      return;
    }
    try {
      final msg = await ref
          .read(apiProvider)
          .modelRecordDelete(model.id, rec.id);
      ref.read(viewingRecordIdProvider.notifier).state = null;
      refreshProjectExtras(ref);
      if (context.mounted) _ok(context, msg);
    } catch (e) {
      if (context.mounted) _ok(context, '$e', color: context.muninn.error);
    }
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    if (recordId == null) {
      return Center(
        child: Text('Pick a record', style: TextStyle(color: m.muted)),
      );
    }
    return ref
        .watch(recordDetailProvider)
        .when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Padding(
            padding: const EdgeInsets.all(16),
            child: Text('$e', style: TextStyle(color: m.error)),
          ),
          data: (rec) {
            if (rec == null) {
              return Center(
                child: Text('Pick a record', style: TextStyle(color: m.muted)),
              );
            }
            return ListView(
              key: ValueKey('record-detail-${rec.id}'),
              padding: const EdgeInsets.all(16),
              children: [
                Text(
                  rec.shortId,
                  style: Theme.of(context).textTheme.titleMedium,
                ),
                const SizedBox(height: 8),
                Wrap(
                  spacing: 8,
                  children: [
                    FilledButton.icon(
                      key: const ValueKey('model-record-edit'),
                      onPressed: () => _edit(context, ref, rec),
                      icon: const Icon(Icons.edit_outlined),
                      label: const Text('Edit…'),
                    ),
                    OutlinedButton.icon(
                      key: const ValueKey('model-record-delete'),
                      onPressed: () => _delete(context, ref, rec),
                      icon: const Icon(Icons.delete_outline),
                      label: const Text('Delete'),
                    ),
                  ],
                ),
                const SizedBox(height: 12),
                for (final f in rec.fields)
                  ListTile(
                    key: ValueKey('record-field-${f.name}'),
                    dense: true,
                    contentPadding: EdgeInsets.zero,
                    title: Text(f.name, style: TextStyle(color: m.muted)),
                    subtitle: Text(f.value.isEmpty ? '—' : f.value),
                  ),
              ],
            );
          },
        );
  }
}
