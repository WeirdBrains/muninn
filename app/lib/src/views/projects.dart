/// Projects: create, rename, describe, delete, attach or detach series, plus
/// journals, live searches, reports, code spaces, live pipes, pipelines and
/// archive import/export. Opening a series shows it in Data with the project's
/// saved time range. Writes reuse the unlocked session (`open_write`); the key
/// stays in Rust.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../state.dart';
import '../theme/muninn_theme.dart';
import '../time_range.dart';
import 'collections.dart';
import 'project_compute.dart';
import 'schedules_plugins.dart';
import 'project_collab.dart';
import 'models.dart';
import 'project_extras.dart';
import 'unlock.dart';

Future<void> showNewProjectDialog(BuildContext context, WidgetRef ref) async {
  ref.read(sectionProvider.notifier).state = Section.projects;
  if (!ref.read(unlockedProvider)) return;
  final created = await showDialog<Project>(
    context: context,
    builder: (ctx) => const _ProjectFieldsDialog(kind: _ProjectEdit.create),
  );
  if (created == null || !context.mounted) return;
  ref.read(selectedProjectProvider.notifier).state = created;
  ref.invalidate(overviewProvider);
}

class ProjectsView extends ConsumerWidget {
  const ProjectsView({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final pane = ref.watch(projectPaneProvider);
    return Unlocked(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(12, 8, 12, 0),
            child: Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                for (final p in ProjectPane.values)
                  FilterChip(
                    key: ValueKey('projects-pane-${p.name}'),
                    label: Text(_paneTitle(p)),
                    selected: pane == p,
                    onSelected: (_) =>
                        ref.read(projectPaneProvider.notifier).state = p,
                  ),
              ],
            ),
          ),
          Expanded(
            child: switch (pane) {
              ProjectPane.projects => const _ProjectsList(),
              ProjectPane.journals => const JournalsPane(),
              ProjectPane.searches => const SearchesPane(),
              ProjectPane.reports => const ReportsPane(),
              ProjectPane.models => const ModelsPane(),
              ProjectPane.code => const CodeSpacesPane(),
              ProjectPane.spaces => const CollectionsPane(),
              ProjectPane.pipes => const PipesPane(),
              ProjectPane.pipelines => const PipelinesPane(),
              ProjectPane.plugins => const PluginsPane(),
              ProjectPane.rules => const RulesPane(),
              ProjectPane.rulesets => const RulesetsPane(),
              ProjectPane.schedules => const SchedulesPane(),
              ProjectPane.reviews => const ReviewsPane(),
              ProjectPane.notifications => const NotificationsPane(),
            },
          ),
        ],
      ),
    );
  }

  static String _paneTitle(ProjectPane p) => switch (p) {
    ProjectPane.projects => 'Projects',
    ProjectPane.journals => 'Journals',
    ProjectPane.searches => 'Searches',
    ProjectPane.reports => 'Reports',
    ProjectPane.models => 'Models',
    ProjectPane.code => 'Code',
    ProjectPane.spaces => 'Spaces',
    ProjectPane.pipes => 'Pipes',
    ProjectPane.pipelines => 'Pipelines',
    ProjectPane.plugins => 'Plugins',
    ProjectPane.rules => 'Rules',
    ProjectPane.rulesets => 'Rule sets',
    ProjectPane.schedules => 'Schedules',
    ProjectPane.reviews => 'Reviews',
    ProjectPane.notifications => 'Notifications',
  };
}

class _ProjectsList extends ConsumerWidget {
  const _ProjectsList();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final sel = ref.watch(selectedProjectProvider);
    return ref
        .watch(overviewProvider)
        .when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Text('$e', style: TextStyle(color: m.error)),
          data: (o) {
            final ps = o?.projects ?? const <Project>[];
            return Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Padding(
                  padding: const EdgeInsets.fromLTRB(12, 8, 12, 0),
                  child: Wrap(
                    spacing: 8,
                    runSpacing: 8,
                    children: [
                      FilledButton.icon(
                        key: const ValueKey('projects-new'),
                        onPressed: () => showNewProjectDialog(context, ref),
                        icon: const Icon(Icons.add),
                        label: const Text('New project'),
                      ),
                      OutlinedButton.icon(
                        key: const ValueKey('projects-import'),
                        onPressed: () => showImportProjectDialog(context, ref),
                        icon: const Icon(Icons.file_upload_outlined),
                        label: const Text('Import archive'),
                      ),
                    ],
                  ),
                ),
                if (ps.isEmpty)
                  Padding(
                    padding: const EdgeInsets.all(16),
                    child: Text(
                      'No projects yet',
                      style: TextStyle(color: m.muted),
                    ),
                  )
                else
                  Expanded(
                    child: ListView(
                      padding: const EdgeInsets.all(8),
                      children: [
                        for (final p in ps) ...[
                          ListTile(
                            minTileHeight: 56,
                            leading: Icon(Icons.folder, color: m.accent),
                            title: Text(p.name),
                            subtitle: p.description.isEmpty
                                ? null
                                : Text(p.description),
                            trailing: Row(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                Icon(
                                  sel?.id == p.id
                                      ? Icons.expand_less
                                      : Icons.expand_more,
                                ),
                                PopupMenuButton<String>(
                                  key: ValueKey('project-menu-${p.id}'),
                                  tooltip: 'Project actions',
                                  onSelected: (v) =>
                                      _onProjectAction(context, ref, p, v),
                                  itemBuilder: (ctx) => const [
                                    PopupMenuItem(
                                      value: 'rename',
                                      child: Text('Rename…'),
                                    ),
                                    PopupMenuItem(
                                      value: 'describe',
                                      child: Text('Edit description…'),
                                    ),
                                    PopupMenuItem(
                                      value: 'delete',
                                      child: Text('Delete…'),
                                    ),
                                    PopupMenuItem(
                                      value: 'export',
                                      child: Text('Export…'),
                                    ),
                                    PopupMenuItem(
                                      value: 'journal',
                                      child: Text('New journal…'),
                                    ),
                                    PopupMenuItem(
                                      value: 'add-item',
                                      child: Text('Add item…'),
                                    ),
                                  ],
                                ),
                              ],
                            ),
                            onTap: () =>
                                ref
                                    .read(selectedProjectProvider.notifier)
                                    .state = sel?.id == p.id
                                ? null
                                : p,
                          ),
                          if (sel?.id == p.id) _ProjectSeriesList(p, o!),
                        ],
                      ],
                    ),
                  ),
              ],
            );
          },
        );
  }
}

Future<void> _onProjectAction(
  BuildContext context,
  WidgetRef ref,
  Project p,
  String action,
) async {
  switch (action) {
    case 'rename':
      final updated = await showDialog<Project>(
        context: context,
        builder: (ctx) =>
            _ProjectFieldsDialog(kind: _ProjectEdit.rename, project: p),
      );
      if (updated == null) return;
      if (ref.read(selectedProjectProvider)?.id == p.id) {
        ref.read(selectedProjectProvider.notifier).state = updated;
      }
      ref.invalidate(overviewProvider);
    case 'describe':
      final updated = await showDialog<Project>(
        context: context,
        builder: (ctx) =>
            _ProjectFieldsDialog(kind: _ProjectEdit.describe, project: p),
      );
      if (updated == null) return;
      if (ref.read(selectedProjectProvider)?.id == p.id) {
        ref.read(selectedProjectProvider.notifier).state = updated;
      }
      ref.invalidate(overviewProvider);
    case 'delete':
      final ok = await showDialog<bool>(
        context: context,
        builder: (ctx) => _DeleteProjectDialog(project: p),
      );
      if (ok != true) return;
      if (ref.read(selectedProjectProvider)?.id == p.id) {
        ref.read(selectedProjectProvider.notifier).state = null;
      }
      ref.invalidate(overviewProvider);
      ref.invalidate(projectSeriesProvider);
      ref.invalidate(projectItemsProvider);
    case 'export':
      await showExportProjectDialog(context, ref, p);
    case 'journal':
      await showNewJournalDialog(context, ref, project: p);
    case 'add-item':
      await showAddProjectItemDialog(context, ref, p);
  }
}

enum _ProjectEdit { create, rename, describe }

class _ProjectFieldsDialog extends ConsumerStatefulWidget {
  const _ProjectFieldsDialog({required this.kind, this.project});
  final _ProjectEdit kind;
  final Project? project;

  @override
  ConsumerState<_ProjectFieldsDialog> createState() =>
      _ProjectFieldsDialogState();
}

class _ProjectFieldsDialogState extends ConsumerState<_ProjectFieldsDialog> {
  late final TextEditingController name;
  late final TextEditingController description;
  bool running = false;
  String? error;

  @override
  void initState() {
    super.initState();
    name = TextEditingController(text: widget.project?.name ?? '');
    description = TextEditingController(
      text: widget.project?.description ?? '',
    );
  }

  @override
  void dispose() {
    name.dispose();
    description.dispose();
    super.dispose();
  }

  String get title => switch (widget.kind) {
    _ProjectEdit.create => 'New project',
    _ProjectEdit.rename => 'Rename project',
    _ProjectEdit.describe => 'Edit description',
  };

  Future<void> _save() async {
    if (!ref.read(unlockedProvider)) {
      setState(() => error = 'Unlock the store first.');
      return;
    }
    final n = name.text.trim();
    final d = description.text;
    if (widget.kind != _ProjectEdit.describe && n.isEmpty) {
      setState(() => error = 'Name is required.');
      return;
    }
    setState(() {
      running = true;
      error = null;
    });
    try {
      final api = ref.read(apiProvider);
      final Project result;
      switch (widget.kind) {
        case _ProjectEdit.create:
          result = await api.createProject(n, description: d);
        case _ProjectEdit.rename:
          result = await api.updateProject(widget.project!.id, name: n);
        case _ProjectEdit.describe:
          result = await api.updateProject(widget.project!.id, description: d);
      }
      if (mounted) Navigator.pop(context, result);
    } catch (e) {
      if (mounted) setState(() => error = e.toString());
    } finally {
      if (mounted) setState(() => running = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    final showName = widget.kind != _ProjectEdit.describe;
    final showDescription = widget.kind != _ProjectEdit.rename;
    return AlertDialog(
      title: Row(
        children: [
          Expanded(
            child: Text(
              title,
              style: TextStyle(color: m.header, fontWeight: FontWeight.w800),
            ),
          ),
          IconButton(
            tooltip: 'Close',
            icon: const Text('[X]'),
            onPressed: () => Navigator.pop(context),
          ),
        ],
      ),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            if (showName) ...[
              TextField(
                key: const ValueKey('project-name'),
                controller: name,
                autofocus: true,
                decoration: const InputDecoration(labelText: 'Name'),
                onSubmitted: (_) => showDescription ? null : _save(),
              ),
              const SizedBox(height: 12),
            ],
            if (showDescription)
              TextField(
                key: const ValueKey('project-description'),
                controller: description,
                autofocus: !showName,
                minLines: 2,
                maxLines: 4,
                decoration: const InputDecoration(labelText: 'Description'),
              ),
            if (error != null) ...[
              const SizedBox(height: 12),
              Text(error!, style: TextStyle(color: m.error)),
            ],
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: running ? null : () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey('project-save'),
          onPressed: running ? null : _save,
          child: running
              ? const SizedBox(
                  width: 16,
                  height: 16,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : const Text('Save'),
        ),
      ],
    );
  }
}

class _DeleteProjectDialog extends ConsumerStatefulWidget {
  const _DeleteProjectDialog({required this.project});
  final Project project;

  @override
  ConsumerState<_DeleteProjectDialog> createState() =>
      _DeleteProjectDialogState();
}

class _DeleteProjectDialogState extends ConsumerState<_DeleteProjectDialog> {
  bool running = false;
  String? error;

  Future<void> _delete() async {
    if (!ref.read(unlockedProvider)) {
      setState(() => error = 'Unlock the store first.');
      return;
    }
    setState(() {
      running = true;
      error = null;
    });
    try {
      await ref.read(apiProvider).removeProject(widget.project.id);
      if (mounted) Navigator.pop(context, true);
    } catch (e) {
      if (mounted) setState(() => error = e.toString());
    } finally {
      if (mounted) setState(() => running = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      title: Row(
        children: [
          Expanded(
            child: Text(
              'Delete project',
              style: TextStyle(color: m.header, fontWeight: FontWeight.w800),
            ),
          ),
          IconButton(
            tooltip: 'Close',
            icon: const Text('[X]'),
            onPressed: () => Navigator.pop(context),
          ),
        ],
      ),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            'Delete “${widget.project.name}”? Journals stay as standalone journals.',
          ),
          if (error != null) ...[
            const SizedBox(height: 12),
            Text(error!, style: TextStyle(color: m.error)),
          ],
        ],
      ),
      actions: [
        TextButton(
          onPressed: running ? null : () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey('project-delete-confirm'),
          onPressed: running ? null : _delete,
          style: FilledButton.styleFrom(backgroundColor: m.error),
          child: const Text('Delete'),
        ),
      ],
    );
  }
}

class _ProjectSeriesList extends ConsumerWidget {
  const _ProjectSeriesList(this.project, this.o);
  final Project project;
  final Overview o;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final d = ref.watch(selectedDatasetValueProvider);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(72, 0, 16, 8),
          child: Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              OutlinedButton.icon(
                key: const ValueKey('project-add-series'),
                onPressed: () => _addCurrent(context, ref),
                icon: const Icon(Icons.add_chart),
                label: Text(
                  d == null
                      ? 'Add current dataset'
                      : 'Add ${d.symbol} · ${d.duration}',
                ),
              ),
              OutlinedButton.icon(
                key: const ValueKey('project-new-journal'),
                onPressed: () =>
                    showNewJournalDialog(context, ref, project: project),
                icon: const Icon(Icons.menu_book),
                label: const Text('New journal'),
              ),
              OutlinedButton.icon(
                key: const ValueKey('project-add-item'),
                onPressed: () =>
                    showAddProjectItemDialog(context, ref, project),
                icon: const Icon(Icons.playlist_add),
                label: const Text('Add item'),
              ),
              OutlinedButton.icon(
                key: const ValueKey('project-export'),
                onPressed: () => showExportProjectDialog(context, ref, project),
                icon: const Icon(Icons.file_download_outlined),
                label: const Text('Export'),
              ),
            ],
          ),
        ),
        ref
            .watch(projectSeriesProvider)
            .when(
              loading: () => const LinearProgressIndicator(),
              error: (e, _) => Padding(
                padding: const EdgeInsets.fromLTRB(72, 0, 16, 12),
                child: Text('$e', style: TextStyle(color: m.error)),
              ),
              data: (items) {
                if (items == null || items.isEmpty) {
                  return Padding(
                    padding: const EdgeInsets.fromLTRB(72, 0, 16, 12),
                    child: Text(
                      'No series in this project yet',
                      style: TextStyle(color: m.muted),
                    ),
                  );
                }
                return Column(
                  children: [
                    for (final s in items)
                      ListTile(
                        minTileHeight: 48,
                        contentPadding: const EdgeInsets.only(
                          left: 72,
                          right: 8,
                        ),
                        leading: Icon(Icons.candlestick_chart, color: m.up),
                        title: Text('${s.source} · ${s.base}/${s.quote}'),
                        subtitle: Text(
                          s.from == null && s.to == null
                              ? 'all time'
                              : '${s.from == null ? '…' : fmtUtc(s.from!)} → ${s.to == null ? '…' : fmtUtc(s.to!)} UTC',
                        ),
                        trailing: IconButton(
                          key: ValueKey('project-remove-series-${s.itemId}'),
                          tooltip: 'Remove from project',
                          icon: Icon(
                            Icons.remove_circle_outline,
                            color: m.muted,
                          ),
                          onPressed: () => _removeSeries(context, ref, s),
                        ),
                        onTap: () {
                          ref.read(timeRangeProvider.notifier).state =
                              TimeRange(from: s.from, to: s.to);
                          ref.read(selectedDatasetProvider.notifier).state =
                              s.key;
                          ref.read(sectionProvider.notifier).state =
                              Section.data;
                        },
                      ),
                  ],
                );
              },
            ),
        ref
            .watch(projectItemsProvider)
            .when(
              loading: () => const SizedBox.shrink(),
              error: (e, _) => Padding(
                padding: const EdgeInsets.fromLTRB(72, 0, 16, 12),
                child: Text('$e', style: TextStyle(color: m.error)),
              ),
              data: (items) {
                final extra = [
                  for (final i in items ?? const <ProjectItemInfo>[])
                    if (!i.isResult) i,
                ];
                if (extra.isEmpty) return const SizedBox.shrink();
                return Column(
                  children: [
                    for (final i in extra)
                      ListTile(
                        minTileHeight: 48,
                        contentPadding: const EdgeInsets.only(
                          left: 72,
                          right: 8,
                        ),
                        leading: Icon(_kindIcon(i.kind), color: m.accent),
                        title: Text(i.name),
                        subtitle: Text(
                          i.detail.isEmpty ? i.kind : '${i.kind} · ${i.detail}',
                        ),
                        trailing: IconButton(
                          key: ValueKey('project-remove-item-${i.itemId}'),
                          tooltip: 'Remove from project',
                          icon: Icon(
                            Icons.remove_circle_outline,
                            color: m.muted,
                          ),
                          onPressed: () => _removeItem(context, ref, i.itemId),
                        ),
                      ),
                  ],
                );
              },
            ),
      ],
    );
  }

  IconData _kindIcon(String kind) => switch (kind) {
    'journal' => Icons.menu_book,
    'live_search' || 'search' => Icons.search,
    'report' => Icons.description,
    'rule' => Icons.rule,
    'model' => Icons.schema,
    'code' => Icons.code,
    _ => Icons.folder_open,
  };

  Future<void> _removeItem(
    BuildContext context,
    WidgetRef ref,
    int itemId,
  ) async {
    final m = context.muninn;
    try {
      await ref.read(apiProvider).removeProjectSeries(itemId);
      ref.invalidate(projectSeriesProvider);
      ref.invalidate(projectItemsProvider);
    } catch (e) {
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('$e', style: TextStyle(color: m.fg)),
          backgroundColor: m.error,
        ),
      );
    }
  }

  Future<void> _addCurrent(BuildContext context, WidgetRef ref) async {
    final m = context.muninn;
    final d = ref.read(selectedDatasetValueProvider);
    if (d == null) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            'Pick a dataset in Data first',
            style: TextStyle(color: m.fg),
          ),
          backgroundColor: m.warning,
        ),
      );
      return;
    }
    if (!ref.read(unlockedProvider)) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            'Unlock the store first.',
            style: TextStyle(color: m.fg),
          ),
          backgroundColor: m.error,
        ),
      );
      return;
    }
    try {
      final added = await ref
          .read(apiProvider)
          .addProjectSeries(project.id, d, ref.read(timeRangeProvider));
      ref.invalidate(projectSeriesProvider);
      ref.invalidate(projectItemsProvider);
      ref.invalidate(overviewProvider);
      if (!context.mounted) return;
      if (!added) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(
              'Already in this project',
              style: TextStyle(color: m.fg),
            ),
            backgroundColor: m.muted,
          ),
        );
      }
    } catch (e) {
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('$e', style: TextStyle(color: m.fg)),
          backgroundColor: m.error,
        ),
      );
    }
  }

  Future<void> _removeSeries(
    BuildContext context,
    WidgetRef ref,
    ProjectSeries s,
  ) async {
    final m = context.muninn;
    try {
      await ref.read(apiProvider).removeProjectSeries(s.itemId);
      ref.invalidate(projectSeriesProvider);
      ref.invalidate(projectItemsProvider);
    } catch (e) {
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('$e', style: TextStyle(color: m.fg)),
          backgroundColor: m.error,
        ),
      );
    }
  }
}
