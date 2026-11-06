/// Model and record viewer (TUI Data ▸ Model designer… / `D`).
/// Schema edits, migrations and rules stay in the TUI and CLI.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../state.dart';
import '../theme/muninn_theme.dart';

Future<void> openModelsPane(WidgetRef ref, {String? modelId, String? recordId}) async {
  ref.read(sectionProvider.notifier).state = Section.projects;
  ref.read(projectPaneProvider.notifier).state = ProjectPane.models;
  ref.read(viewingModelIdProvider.notifier).state = modelId;
  ref.read(viewingRecordIdProvider.notifier).state = recordId;
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
            if (all.isEmpty) {
              return Padding(
                padding: const EdgeInsets.all(16),
                child: Text(
                  'No models yet. Create one in the TUI (Data ▸ Model designer…) or `mnnn model`.',
                  style: TextStyle(color: m.muted),
                ),
              );
            }
            ModelInfo? selected;
            for (final x in all) {
              if (x.id == modelId || x.name == modelId || x.shortId == modelId) {
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
                      for (final model in all)
                        ListTile(
                          key: ValueKey('model-${model.id}'),
                          selected: selected?.id == model.id,
                          title: Text(model.name),
                          subtitle: Text(
                            [
                              if (model.description.isNotEmpty) model.description,
                              '${model.recordCount} records',
                              if (model.fields.isNotEmpty) model.fields.join(', '),
                            ].join(' · '),
                            style: TextStyle(color: m.muted),
                          ),
                          onTap: () {
                            ref.read(viewingModelIdProvider.notifier).state =
                                model.id;
                            ref.read(viewingRecordIdProvider.notifier).state =
                                null;
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
                      : _RecordsColumn(
                          model: selected,
                          recordId: recordId,
                        ),
                ),
              ],
            );
          },
        );
  }
}

class _RecordsColumn extends ConsumerWidget {
  const _RecordsColumn({required this.model, this.recordId});
  final ModelInfo model;
  final String? recordId;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
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
                          onTap: () => ref
                              .read(viewingRecordIdProvider.notifier)
                              .state = r.id,
                        ),
                    ],
                  ),
                ),
                VerticalDivider(width: 1, color: m.border),
                Expanded(child: _RecordDetail(modelId: model.id, recordId: recordId)),
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
  const _RecordDetail({required this.modelId, this.recordId});
  final String modelId;
  final String? recordId;

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
