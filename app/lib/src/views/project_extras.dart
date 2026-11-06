/// Journals, live searches, reports, and project import/export (`mnnn journal`,
/// `mnnn search`, `mnnn report`, `mnnn project import|export|add`). Writes reuse
/// the unlocked session (`open_write`); the key stays in Rust. Archive bytes
/// never leave Rust: Dart only passes filesystem paths.
library;

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../state.dart';
import '../theme/muninn_theme.dart';

void _ok(BuildContext context, String msg, {Color? color}) {
  ScaffoldMessenger.of(context)
      .showSnackBar(SnackBar(content: Text(msg), backgroundColor: color));
}

Future<void> showImportProjectDialog(
  BuildContext context,
  WidgetRef ref,
) async {
  ref.read(sectionProvider.notifier).state = Section.projects;
  ref.read(projectPaneProvider.notifier).state = ProjectPane.projects;
  if (!ref.read(unlockedProvider)) return;
  final msg = await showDialog<String>(
    context: context,
    builder: (ctx) => const _ImportProjectDialog(),
  );
  if (msg == null || !context.mounted) return;
  refreshProjectExtras(ref);
  _ok(context, msg);
}

Future<void> showExportProjectDialog(
  BuildContext context,
  WidgetRef ref,
  Project project,
) async {
  if (!ref.read(unlockedProvider)) return;
  final msg = await showDialog<String>(
    context: context,
    builder: (ctx) => _ExportProjectDialog(project: project),
  );
  if (msg == null || !context.mounted) return;
  _ok(context, msg);
}

Future<void> showNewJournalDialog(
  BuildContext context,
  WidgetRef ref, {
  Project? project,
}) async {
  ref.read(sectionProvider.notifier).state = Section.projects;
  ref.read(projectPaneProvider.notifier).state = ProjectPane.journals;
  if (!ref.read(unlockedProvider)) return;
  final created = await showDialog<JournalInfo>(
    context: context,
    builder: (ctx) => _JournalFieldsDialog(project: project),
  );
  if (created == null || !context.mounted) return;
  ref.read(selectedJournalProvider.notifier).state = created;
  refreshProjectExtras(ref);
}

Future<void> showSaveSearchDialog(BuildContext context, WidgetRef ref) async {
  ref.read(sectionProvider.notifier).state = Section.projects;
  ref.read(projectPaneProvider.notifier).state = ProjectPane.searches;
  if (!ref.read(unlockedProvider)) return;
  final created = await showDialog<SearchInfo>(
    context: context,
    builder: (ctx) => const _SearchFieldsDialog(),
  );
  if (created == null || !context.mounted) return;
  refreshProjectExtras(ref);
}

Future<void> showNewReportDialog(
  BuildContext context,
  WidgetRef ref, {
  Project? project,
}) async {
  ref.read(sectionProvider.notifier).state = Section.projects;
  ref.read(projectPaneProvider.notifier).state = ProjectPane.reports;
  if (!ref.read(unlockedProvider)) return;
  final created = await showDialog<ReportInfo>(
    context: context,
    builder: (ctx) => _ReportFieldsDialog(project: project),
  );
  if (created == null || !context.mounted) return;
  ref.read(selectedReportProvider.notifier).state = created;
  refreshProjectExtras(ref);
}

Future<void> showAddProjectItemDialog(
  BuildContext context,
  WidgetRef ref,
  Project project,
) async {
  if (!ref.read(unlockedProvider)) return;
  final msg = await showDialog<String>(
    context: context,
    builder: (ctx) => _AddProjectItemDialog(project: project),
  );
  if (msg == null || !context.mounted) return;
  refreshProjectExtras(ref);
  _ok(context, msg);
}

class JournalsPane extends ConsumerWidget {
  const JournalsPane({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final sel = ref.watch(selectedJournalProvider);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(12, 8, 12, 0),
          child: Align(
            alignment: Alignment.centerLeft,
            child: FilledButton.icon(
              key: const ValueKey('journals-new'),
              onPressed: () => showNewJournalDialog(context, ref),
              icon: const Icon(Icons.add),
              label: const Text('New journal'),
            ),
          ),
        ),
        Expanded(
          child: ref
              .watch(journalsProvider)
              .when(
                loading: () => const LinearProgressIndicator(),
                error: (e, _) => Padding(
                  padding: const EdgeInsets.all(16),
                  child: Text('$e', style: TextStyle(color: m.error)),
                ),
                data: (items) {
                  final js = items ?? const <JournalInfo>[];
                  if (js.isEmpty) {
                    return Padding(
                      padding: const EdgeInsets.all(16),
                      child: Text(
                        'No journals yet',
                        style: TextStyle(color: m.muted),
                      ),
                    );
                  }
                  return ListView(
                    padding: const EdgeInsets.all(8),
                    children: [
                      for (final j in js) ...[
                        ListTile(
                          minTileHeight: 56,
                          leading: Icon(Icons.menu_book, color: m.accent),
                          title: Text(j.name),
                          subtitle: Text(
                            [
                              j.format,
                              if (j.projectName.isNotEmpty) j.projectName,
                              '${j.entries} entries',
                            ].join(' · '),
                          ),
                          trailing: Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              Icon(
                                sel?.id == j.id
                                    ? Icons.expand_less
                                    : Icons.expand_more,
                              ),
                              PopupMenuButton<String>(
                                key: ValueKey('journal-menu-${j.id}'),
                                tooltip: 'Journal actions',
                                onSelected: (v) =>
                                    _onJournalAction(context, ref, j, v),
                                itemBuilder: (ctx) => const [
                                  PopupMenuItem(
                                    value: 'rename',
                                    child: Text('Rename…'),
                                  ),
                                  PopupMenuItem(
                                    value: 'format',
                                    child: Text('Change format…'),
                                  ),
                                  PopupMenuItem(
                                    value: 'fork',
                                    child: Text('Fork…'),
                                  ),
                                  PopupMenuItem(
                                    value: 'delete',
                                    child: Text('Delete…'),
                                  ),
                                ],
                              ),
                            ],
                          ),
                          onTap: () =>
                              ref.read(selectedJournalProvider.notifier).state =
                                  sel?.id == j.id ? null : j,
                        ),
                        if (sel?.id == j.id) _JournalDetail(j),
                      ],
                    ],
                  );
                },
              ),
        ),
      ],
    );
  }
}

Future<void> _onJournalAction(
  BuildContext context,
  WidgetRef ref,
  JournalInfo j,
  String action,
) async {
  switch (action) {
    case 'rename':
      final updated = await showDialog<JournalInfo>(
        context: context,
        builder: (ctx) => _JournalFieldsDialog(journal: j, mode: 'rename'),
      );
      if (updated == null) return;
      if (ref.read(selectedJournalProvider)?.id == j.id) {
        ref.read(selectedJournalProvider.notifier).state = updated;
      }
      refreshProjectExtras(ref);
    case 'format':
      final updated = await showDialog<JournalInfo>(
        context: context,
        builder: (ctx) => _JournalFieldsDialog(journal: j, mode: 'format'),
      );
      if (updated == null) return;
      if (ref.read(selectedJournalProvider)?.id == j.id) {
        ref.read(selectedJournalProvider.notifier).state = updated;
      }
      refreshProjectExtras(ref);
    case 'fork':
      final forked = await showDialog<JournalInfo>(
        context: context,
        builder: (ctx) => _JournalFieldsDialog(journal: j, mode: 'fork'),
      );
      if (forked == null) return;
      ref.read(selectedJournalProvider.notifier).state = forked;
      refreshProjectExtras(ref);
    case 'delete':
      final ok = await showDialog<bool>(
        context: context,
        builder: (ctx) => _ConfirmRemoveDialog(
          title: 'Delete journal',
          body: 'Delete “${j.name}” and its entries?',
          confirmKey: 'journal-delete-confirm',
          run: () => ref.read(apiProvider).removeJournal(j.id),
        ),
      );
      if (ok != true) return;
      if (ref.read(selectedJournalProvider)?.id == j.id) {
        ref.read(selectedJournalProvider.notifier).state = null;
      }
      refreshProjectExtras(ref);
  }
}

class _JournalDetail extends ConsumerWidget {
  const _JournalDetail(this.journal);
  final JournalInfo journal;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
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
                key: const ValueKey('journal-add-entry'),
                onPressed: () async {
                  final created = await showDialog<JournalEntryInfo>(
                    context: context,
                    builder: (ctx) => _EntryFieldsDialog(journal: journal),
                  );
                  if (created == null) return;
                  refreshProjectExtras(ref);
                },
                icon: const Icon(Icons.note_add),
                label: const Text('Write entry'),
              ),
              OutlinedButton.icon(
                key: const ValueKey('journal-add-note'),
                onPressed: () async {
                  final created = await showDialog<JournalNoteInfo>(
                    context: context,
                    builder: (ctx) => _NoteFieldsDialog(journal: journal),
                  );
                  if (created == null) return;
                  refreshProjectExtras(ref);
                },
                icon: const Icon(Icons.sticky_note_2_outlined),
                label: const Text('Add note'),
              ),
            ],
          ),
        ),
        ref
            .watch(journalNotesProvider)
            .when(
              loading: () => const SizedBox.shrink(),
              error: (e, _) => Padding(
                padding: const EdgeInsets.fromLTRB(72, 0, 16, 8),
                child: Text('$e', style: TextStyle(color: m.error)),
              ),
              data: (notes) {
                if (notes == null || notes.isEmpty) {
                  return const SizedBox.shrink();
                }
                return Column(
                  children: [
                    for (final n in notes)
                      ListTile(
                        minTileHeight: 40,
                        contentPadding: const EdgeInsets.only(
                          left: 72,
                          right: 16,
                        ),
                        leading: Icon(Icons.sticky_note_2, color: m.warning),
                        title: Text(n.body),
                      ),
                  ],
                );
              },
            ),
        ref
            .watch(journalEntriesProvider)
            .when(
              loading: () => const LinearProgressIndicator(),
              error: (e, _) => Padding(
                padding: const EdgeInsets.fromLTRB(72, 0, 16, 12),
                child: Text('$e', style: TextStyle(color: m.error)),
              ),
              data: (entries) {
                if (entries == null || entries.isEmpty) {
                  return Padding(
                    padding: const EdgeInsets.fromLTRB(72, 0, 16, 12),
                    child: Text(
                      'No entries yet',
                      style: TextStyle(color: m.muted),
                    ),
                  );
                }
                return Column(
                  children: [
                    for (final e in entries)
                      ListTile(
                        minTileHeight: 48,
                        contentPadding: const EdgeInsets.only(
                          left: 72,
                          right: 8,
                        ),
                        leading: Icon(Icons.article_outlined, color: m.up),
                        title: Text(e.heading.isEmpty ? e.title : e.heading),
                        subtitle: e.body.isEmpty
                            ? null
                            : Text(e.body, maxLines: 2),
                        trailing: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            IconButton(
                              key: ValueKey('entry-edit-${e.id}'),
                              tooltip: 'Edit entry',
                              icon: const Icon(Icons.edit_outlined),
                              onPressed: () async {
                                final updated =
                                    await showDialog<JournalEntryInfo>(
                                      context: context,
                                      builder: (ctx) => _EntryFieldsDialog(
                                        journal: journal,
                                        entry: e,
                                      ),
                                    );
                                if (updated == null) return;
                                refreshProjectExtras(ref);
                              },
                            ),
                            IconButton(
                              key: ValueKey('entry-remove-${e.id}'),
                              tooltip: 'Remove entry',
                              icon: Icon(
                                Icons.remove_circle_outline,
                                color: m.muted,
                              ),
                              onPressed: () async {
                                try {
                                  await ref
                                      .read(apiProvider)
                                      .removeJournalEntry(e.id);
                                  refreshProjectExtras(ref);
                                } catch (err) {
                                  if (context.mounted) {
                                    _ok(context, '$err', color: m.error);
                                  }
                                }
                              },
                            ),
                          ],
                        ),
                      ),
                  ],
                );
              },
            ),
      ],
    );
  }
}

class SearchesPane extends ConsumerWidget {
  const SearchesPane({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(12, 8, 12, 0),
          child: Align(
            alignment: Alignment.centerLeft,
            child: FilledButton.icon(
              key: const ValueKey('searches-new'),
              onPressed: () => showSaveSearchDialog(context, ref),
              icon: const Icon(Icons.add),
              label: const Text('Save search'),
            ),
          ),
        ),
        Expanded(
          child: ref
              .watch(searchesProvider)
              .when(
                loading: () => const LinearProgressIndicator(),
                error: (e, _) => Padding(
                  padding: const EdgeInsets.all(16),
                  child: Text('$e', style: TextStyle(color: m.error)),
                ),
                data: (items) {
                  final ss = items ?? const <SearchInfo>[];
                  if (ss.isEmpty) {
                    return Padding(
                      padding: const EdgeInsets.all(16),
                      child: Text(
                        'No live searches yet',
                        style: TextStyle(color: m.muted),
                      ),
                    );
                  }
                  return ListView(
                    padding: const EdgeInsets.all(8),
                    children: [
                      for (final s in ss)
                        ListTile(
                          minTileHeight: 56,
                          leading: Icon(Icons.search, color: m.accent),
                          title: Text(s.name),
                          subtitle: Text(
                            [
                              s.target,
                              if (s.query.isNotEmpty) '"${s.query}"',
                              if (s.describe.isNotEmpty) s.describe,
                            ].join(' · '),
                          ),
                          trailing: PopupMenuButton<String>(
                            key: ValueKey('search-menu-${s.id}'),
                            tooltip: 'Search actions',
                            onSelected: (v) =>
                                _onSearchAction(context, ref, s, v),
                            itemBuilder: (ctx) => const [
                              PopupMenuItem(
                                value: 'rename',
                                child: Text('Rename…'),
                              ),
                              PopupMenuItem(
                                value: 'fork',
                                child: Text('Fork…'),
                              ),
                              PopupMenuItem(
                                value: 'delete',
                                child: Text('Delete…'),
                              ),
                            ],
                          ),
                        ),
                    ],
                  );
                },
              ),
        ),
      ],
    );
  }
}

Future<void> _onSearchAction(
  BuildContext context,
  WidgetRef ref,
  SearchInfo s,
  String action,
) async {
  switch (action) {
    case 'rename':
      final updated = await showDialog<SearchInfo>(
        context: context,
        builder: (ctx) => _SearchFieldsDialog(search: s, mode: 'rename'),
      );
      if (updated == null) return;
      refreshProjectExtras(ref);
    case 'fork':
      final forked = await showDialog<SearchInfo>(
        context: context,
        builder: (ctx) => _SearchFieldsDialog(search: s, mode: 'fork'),
      );
      if (forked == null) return;
      refreshProjectExtras(ref);
    case 'delete':
      final ok = await showDialog<bool>(
        context: context,
        builder: (ctx) => _ConfirmRemoveDialog(
          title: 'Delete live search',
          body: 'Delete “${s.name}”?',
          confirmKey: 'search-delete-confirm',
          run: () => ref.read(apiProvider).removeSearch(s.id),
        ),
      );
      if (ok != true) return;
      refreshProjectExtras(ref);
  }
}

class ReportsPane extends ConsumerWidget {
  const ReportsPane({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final sel = ref.watch(selectedReportProvider);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(12, 8, 12, 0),
          child: Align(
            alignment: Alignment.centerLeft,
            child: FilledButton.icon(
              key: const ValueKey('reports-new'),
              onPressed: () => showNewReportDialog(context, ref),
              icon: const Icon(Icons.add),
              label: const Text('New report'),
            ),
          ),
        ),
        Expanded(
          child: ref
              .watch(reportsProvider)
              .when(
                loading: () => const LinearProgressIndicator(),
                error: (e, _) => Padding(
                  padding: const EdgeInsets.all(16),
                  child: Text('$e', style: TextStyle(color: m.error)),
                ),
                data: (items) {
                  final rs = items ?? const <ReportInfo>[];
                  if (rs.isEmpty) {
                    return Padding(
                      padding: const EdgeInsets.all(16),
                      child: Text(
                        'No reports yet',
                        style: TextStyle(color: m.muted),
                      ),
                    );
                  }
                  return ListView(
                    padding: const EdgeInsets.all(8),
                    children: [
                      for (final r in rs) ...[
                        ListTile(
                          minTileHeight: 56,
                          leading: Icon(Icons.description, color: m.accent),
                          title: Text(r.name),
                          subtitle: Text(
                            [
                              if (r.description.isNotEmpty) r.description,
                              '${r.sections} sections',
                            ].join(' · '),
                          ),
                          trailing: Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              Icon(
                                sel?.id == r.id
                                    ? Icons.expand_less
                                    : Icons.expand_more,
                              ),
                              PopupMenuButton<String>(
                                key: ValueKey('report-menu-${r.id}'),
                                tooltip: 'Report actions',
                                onSelected: (v) =>
                                    _onReportAction(context, ref, r, v),
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
                                ],
                              ),
                            ],
                          ),
                          onTap: () =>
                              ref.read(selectedReportProvider.notifier).state =
                                  sel?.id == r.id ? null : r,
                        ),
                        if (sel?.id == r.id) _ReportDetail(r),
                      ],
                    ],
                  );
                },
              ),
        ),
      ],
    );
  }
}

Future<void> _onReportAction(
  BuildContext context,
  WidgetRef ref,
  ReportInfo r,
  String action,
) async {
  switch (action) {
    case 'rename':
      final updated = await showDialog<ReportInfo>(
        context: context,
        builder: (ctx) => _ReportFieldsDialog(report: r, mode: 'rename'),
      );
      if (updated == null) return;
      if (ref.read(selectedReportProvider)?.id == r.id) {
        ref.read(selectedReportProvider.notifier).state = updated;
      }
      refreshProjectExtras(ref);
    case 'describe':
      final updated = await showDialog<ReportInfo>(
        context: context,
        builder: (ctx) => _ReportFieldsDialog(report: r, mode: 'describe'),
      );
      if (updated == null) return;
      if (ref.read(selectedReportProvider)?.id == r.id) {
        ref.read(selectedReportProvider.notifier).state = updated;
      }
      refreshProjectExtras(ref);
    case 'delete':
      final ok = await showDialog<bool>(
        context: context,
        builder: (ctx) => _ConfirmRemoveDialog(
          title: 'Delete report',
          body: 'Delete “${r.name}”?',
          confirmKey: 'report-delete-confirm',
          run: () => ref.read(apiProvider).removeReport(r.id),
        ),
      );
      if (ok != true) return;
      if (ref.read(selectedReportProvider)?.id == r.id) {
        ref.read(selectedReportProvider.notifier).state = null;
      }
      refreshProjectExtras(ref);
  }
}

class _ReportDetail extends ConsumerWidget {
  const _ReportDetail(this.report);
  final ReportInfo report;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(72, 0, 16, 8),
          child: Align(
            alignment: Alignment.centerLeft,
            child: OutlinedButton.icon(
              key: const ValueKey('report-add-section'),
              onPressed: () async {
                final created = await showDialog<ReportSectionInfo>(
                  context: context,
                  builder: (ctx) => _SectionFieldsDialog(report: report),
                );
                if (created == null) return;
                refreshProjectExtras(ref);
              },
              icon: const Icon(Icons.add),
              label: const Text('Add section'),
            ),
          ),
        ),
        ref
            .watch(reportSectionsProvider)
            .when(
              loading: () => const LinearProgressIndicator(),
              error: (e, _) => Padding(
                padding: const EdgeInsets.fromLTRB(72, 0, 16, 12),
                child: Text('$e', style: TextStyle(color: m.error)),
              ),
              data: (sections) {
                if (sections == null || sections.isEmpty) {
                  return Padding(
                    padding: const EdgeInsets.fromLTRB(72, 0, 16, 12),
                    child: Text(
                      'No sections yet',
                      style: TextStyle(color: m.muted),
                    ),
                  );
                }
                return Column(
                  children: [
                    for (final s in sections)
                      ListTile(
                        minTileHeight: 48,
                        contentPadding: const EdgeInsets.only(
                          left: 72,
                          right: 8,
                        ),
                        leading: Icon(Icons.view_agenda_outlined, color: m.up),
                        title: Text(
                          s.heading.isEmpty ? '${s.kind} section' : s.heading,
                        ),
                        subtitle: Text(
                          [s.kind, if (s.body.isNotEmpty) s.body].join(' · '),
                          maxLines: 2,
                        ),
                        trailing: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            IconButton(
                              key: ValueKey('section-up-${s.id}'),
                              tooltip: 'Move up',
                              icon: const Icon(Icons.arrow_upward),
                              onPressed: s.position <= 1
                                  ? null
                                  : () async {
                                      try {
                                        await ref
                                            .read(apiProvider)
                                            .moveReportSection(
                                              report.id,
                                              s.id,
                                              s.position - 1,
                                            );
                                        refreshProjectExtras(ref);
                                      } catch (err) {
                                        if (context.mounted) {
                                          _ok(context, '$err', color: m.error);
                                        }
                                      }
                                    },
                            ),
                            IconButton(
                              key: ValueKey('section-down-${s.id}'),
                              tooltip: 'Move down',
                              icon: const Icon(Icons.arrow_downward),
                              onPressed: () async {
                                try {
                                  await ref
                                      .read(apiProvider)
                                      .moveReportSection(
                                        report.id,
                                        s.id,
                                        s.position + 1,
                                      );
                                  refreshProjectExtras(ref);
                                } catch (err) {
                                  if (context.mounted) {
                                    _ok(context, '$err', color: m.error);
                                  }
                                }
                              },
                            ),
                            IconButton(
                              key: ValueKey('section-edit-${s.id}'),
                              tooltip: 'Edit section',
                              icon: const Icon(Icons.edit_outlined),
                              onPressed: () async {
                                final updated =
                                    await showDialog<ReportSectionInfo>(
                                      context: context,
                                      builder: (ctx) => _SectionFieldsDialog(
                                        report: report,
                                        section: s,
                                      ),
                                    );
                                if (updated == null) return;
                                refreshProjectExtras(ref);
                              },
                            ),
                            IconButton(
                              key: ValueKey('section-remove-${s.id}'),
                              tooltip: 'Remove section',
                              icon: Icon(
                                Icons.remove_circle_outline,
                                color: m.muted,
                              ),
                              onPressed: () async {
                                try {
                                  await ref
                                      .read(apiProvider)
                                      .removeReportSection(report.id, s.id);
                                  refreshProjectExtras(ref);
                                } catch (err) {
                                  if (context.mounted) {
                                    _ok(context, '$err', color: m.error);
                                  }
                                }
                              },
                            ),
                          ],
                        ),
                      ),
                  ],
                );
              },
            ),
      ],
    );
  }
}

class _JournalFieldsDialog extends ConsumerStatefulWidget {
  const _JournalFieldsDialog({
    this.journal,
    this.project,
    this.mode = 'create',
  });
  final JournalInfo? journal;
  final Project? project;
  final String mode;

  @override
  ConsumerState<_JournalFieldsDialog> createState() =>
      _JournalFieldsDialogState();
}

class _JournalFieldsDialogState extends ConsumerState<_JournalFieldsDialog> {
  late final TextEditingController name;
  late String format;
  bool running = false;
  String? error;

  @override
  void initState() {
    super.initState();
    final existing = widget.journal;
    name = TextEditingController(
      text: widget.mode == 'fork'
          ? '${existing?.name ?? ''} (fork)'
          : existing?.name ?? '',
    );
    format = existing?.format ?? 'markdown';
  }

  @override
  void dispose() {
    name.dispose();
    super.dispose();
  }

  String get title => switch (widget.mode) {
    'rename' => 'Rename journal',
    'format' => 'Change format',
    'fork' => 'Fork journal',
    _ => 'New journal',
  };

  Future<void> _save() async {
    if (!ref.read(unlockedProvider)) {
      setState(() => error = 'Unlock the store first.');
      return;
    }
    final n = name.text.trim();
    if (widget.mode != 'format' && n.isEmpty) {
      setState(() => error = 'Name is required.');
      return;
    }
    setState(() {
      running = true;
      error = null;
    });
    try {
      final api = ref.read(apiProvider);
      final JournalInfo result;
      switch (widget.mode) {
        case 'rename':
          result = await api.updateJournal(widget.journal!.id, name: n);
        case 'format':
          result = await api.updateJournal(widget.journal!.id, format: format);
        case 'fork':
          result = await api.forkJournal(
            widget.journal!.id,
            name: n,
            project: widget.project?.id,
          );
        default:
          result = await api.createJournal(
            n,
            project: widget.project?.id,
            format: format,
          );
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
    final showName = widget.mode != 'format';
    final showFormat = widget.mode == 'create' || widget.mode == 'format';
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
            if (showName)
              TextField(
                key: const ValueKey('journal-name'),
                controller: name,
                autofocus: true,
                decoration: const InputDecoration(labelText: 'Name'),
                onSubmitted: (_) => showFormat ? null : _save(),
              ),
            if (showFormat) ...[
              const SizedBox(height: 12),
              DropdownButtonFormField<String>(
                key: const ValueKey('journal-format'),
                initialValue: format,
                decoration: const InputDecoration(labelText: 'Format'),
                items: const [
                  DropdownMenuItem(value: 'markdown', child: Text('markdown')),
                  DropdownMenuItem(value: 'latex', child: Text('latex')),
                ],
                onChanged: (v) {
                  if (v != null) setState(() => format = v);
                },
              ),
            ],
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
          key: const ValueKey('journal-save'),
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

class _EntryFieldsDialog extends ConsumerStatefulWidget {
  const _EntryFieldsDialog({required this.journal, this.entry});
  final JournalInfo journal;
  final JournalEntryInfo? entry;

  @override
  ConsumerState<_EntryFieldsDialog> createState() => _EntryFieldsDialogState();
}

class _EntryFieldsDialogState extends ConsumerState<_EntryFieldsDialog> {
  late final TextEditingController title;
  late final TextEditingController body;
  late String format;
  bool running = false;
  String? error;

  @override
  void initState() {
    super.initState();
    title = TextEditingController(text: widget.entry?.title ?? '');
    body = TextEditingController(text: widget.entry?.body ?? '');
    format = widget.entry?.format ?? widget.journal.format;
  }

  @override
  void dispose() {
    title.dispose();
    body.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    if (!ref.read(unlockedProvider)) {
      setState(() => error = 'Unlock the store first.');
      return;
    }
    setState(() {
      running = true;
      error = null;
    });
    try {
      final api = ref.read(apiProvider);
      final JournalEntryInfo result;
      if (widget.entry == null) {
        result = await api.addJournalEntry(
          widget.journal.id,
          title: title.text.trim(),
          body: body.text,
          format: format,
        );
      } else {
        result = await api.updateJournalEntry(
          widget.entry!.id,
          title: title.text.trim(),
          body: body.text,
          format: format,
        );
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
    return AlertDialog(
      title: Row(
        children: [
          Expanded(
            child: Text(
              widget.entry == null ? 'Write entry' : 'Edit entry',
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
        width: 480,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            TextField(
              key: const ValueKey('entry-title'),
              controller: title,
              autofocus: true,
              decoration: const InputDecoration(labelText: 'Title'),
            ),
            const SizedBox(height: 12),
            TextField(
              key: const ValueKey('entry-body'),
              controller: body,
              minLines: 4,
              maxLines: 8,
              decoration: const InputDecoration(labelText: 'Body'),
            ),
            const SizedBox(height: 12),
            DropdownButtonFormField<String>(
              initialValue: format,
              decoration: const InputDecoration(labelText: 'Format'),
              items: const [
                DropdownMenuItem(value: 'markdown', child: Text('markdown')),
                DropdownMenuItem(value: 'latex', child: Text('latex')),
              ],
              onChanged: (v) {
                if (v != null) setState(() => format = v);
              },
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
          key: const ValueKey('entry-save'),
          onPressed: running ? null : _save,
          child: const Text('Save'),
        ),
      ],
    );
  }
}

class _NoteFieldsDialog extends ConsumerStatefulWidget {
  const _NoteFieldsDialog({required this.journal});
  final JournalInfo journal;

  @override
  ConsumerState<_NoteFieldsDialog> createState() => _NoteFieldsDialogState();
}

class _NoteFieldsDialogState extends ConsumerState<_NoteFieldsDialog> {
  final body = TextEditingController();
  bool running = false;
  String? error;

  @override
  void dispose() {
    body.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final text = body.text.trim();
    if (text.isEmpty) {
      setState(() => error = 'Note is required.');
      return;
    }
    setState(() {
      running = true;
      error = null;
    });
    try {
      final n = await ref
          .read(apiProvider)
          .addJournalNote(widget.journal.id, text);
      if (mounted) Navigator.pop(context, n);
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
              'Add note',
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
          children: [
            TextField(
              key: const ValueKey('note-body'),
              controller: body,
              autofocus: true,
              minLines: 2,
              maxLines: 4,
              decoration: const InputDecoration(labelText: 'Note'),
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
          key: const ValueKey('note-save'),
          onPressed: running ? null : _save,
          child: const Text('Save'),
        ),
      ],
    );
  }
}

class _SearchFieldsDialog extends ConsumerStatefulWidget {
  const _SearchFieldsDialog({this.search, this.mode = 'create'});
  final SearchInfo? search;
  final String mode;

  @override
  ConsumerState<_SearchFieldsDialog> createState() =>
      _SearchFieldsDialogState();
}

class _SearchFieldsDialogState extends ConsumerState<_SearchFieldsDialog> {
  late final TextEditingController name;
  late final TextEditingController query;
  late String target;
  bool running = false;
  String? error;

  @override
  void initState() {
    super.initState();
    name = TextEditingController(
      text: widget.mode == 'fork'
          ? '${widget.search?.name ?? ''} (fork)'
          : widget.search?.name ?? '',
    );
    query = TextEditingController(text: widget.search?.query ?? '');
    target = widget.search?.target ?? 'series';
  }

  @override
  void dispose() {
    name.dispose();
    query.dispose();
    super.dispose();
  }

  String get title => switch (widget.mode) {
    'rename' => 'Rename live search',
    'fork' => 'Fork live search',
    _ => 'Save search as live search',
  };

  Future<void> _save() async {
    if (!ref.read(unlockedProvider)) {
      setState(() => error = 'Unlock the store first.');
      return;
    }
    final n = name.text.trim();
    if (n.isEmpty) {
      setState(() => error = 'Name is required.');
      return;
    }
    setState(() {
      running = true;
      error = null;
    });
    try {
      final api = ref.read(apiProvider);
      final SearchInfo result;
      switch (widget.mode) {
        case 'rename':
          result = await api.renameSearch(widget.search!.id, n);
        case 'fork':
          result = await api.forkSearch(widget.search!.id, name: n);
        default:
          result = await api.saveSearch(
            n,
            query: query.text.trim(),
            target: target,
          );
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
    final create = widget.mode == 'create';
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
            TextField(
              key: const ValueKey('search-name'),
              controller: name,
              autofocus: true,
              decoration: const InputDecoration(labelText: 'Name'),
            ),
            if (create) ...[
              const SizedBox(height: 12),
              TextField(
                key: const ValueKey('search-query'),
                controller: query,
                decoration: const InputDecoration(labelText: 'Query'),
              ),
              const SizedBox(height: 12),
              DropdownButtonFormField<String>(
                key: const ValueKey('search-target'),
                initialValue: target,
                decoration: const InputDecoration(labelText: 'Target'),
                items: const [
                  DropdownMenuItem(value: 'series', child: Text('series')),
                  DropdownMenuItem(value: 'journals', child: Text('journals')),
                  DropdownMenuItem(value: 'records', child: Text('records')),
                ],
                onChanged: (v) {
                  if (v != null) setState(() => target = v);
                },
              ),
            ],
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
          key: const ValueKey('search-save'),
          onPressed: running ? null : _save,
          child: const Text('Save'),
        ),
      ],
    );
  }
}

class _ReportFieldsDialog extends ConsumerStatefulWidget {
  const _ReportFieldsDialog({this.report, this.project, this.mode = 'create'});
  final ReportInfo? report;
  final Project? project;
  final String mode;

  @override
  ConsumerState<_ReportFieldsDialog> createState() =>
      _ReportFieldsDialogState();
}

class _ReportFieldsDialogState extends ConsumerState<_ReportFieldsDialog> {
  late final TextEditingController name;
  late final TextEditingController description;
  bool running = false;
  String? error;

  @override
  void initState() {
    super.initState();
    name = TextEditingController(text: widget.report?.name ?? '');
    description = TextEditingController(text: widget.report?.description ?? '');
  }

  @override
  void dispose() {
    name.dispose();
    description.dispose();
    super.dispose();
  }

  String get title => switch (widget.mode) {
    'rename' => 'Rename report',
    'describe' => 'Edit description',
    _ => 'New report',
  };

  Future<void> _save() async {
    if (!ref.read(unlockedProvider)) {
      setState(() => error = 'Unlock the store first.');
      return;
    }
    final n = name.text.trim();
    if (widget.mode != 'describe' && n.isEmpty) {
      setState(() => error = 'Name is required.');
      return;
    }
    setState(() {
      running = true;
      error = null;
    });
    try {
      final api = ref.read(apiProvider);
      final ReportInfo result;
      switch (widget.mode) {
        case 'rename':
          result = await api.updateReport(widget.report!.id, name: n);
        case 'describe':
          result = await api.updateReport(
            widget.report!.id,
            description: description.text,
          );
        default:
          result = await api.createReport(
            n,
            description: description.text,
            project: widget.project?.id,
          );
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
    final showName = widget.mode != 'describe';
    final showDescription = widget.mode != 'rename';
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
                key: const ValueKey('report-name'),
                controller: name,
                autofocus: true,
                decoration: const InputDecoration(labelText: 'Name'),
              ),
              const SizedBox(height: 12),
            ],
            if (showDescription)
              TextField(
                key: const ValueKey('report-description'),
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
          key: const ValueKey('report-save'),
          onPressed: running ? null : _save,
          child: const Text('Save'),
        ),
      ],
    );
  }
}

class _SectionFieldsDialog extends ConsumerStatefulWidget {
  const _SectionFieldsDialog({required this.report, this.section});
  final ReportInfo report;
  final ReportSectionInfo? section;

  @override
  ConsumerState<_SectionFieldsDialog> createState() =>
      _SectionFieldsDialogState();
}

class _SectionFieldsDialogState extends ConsumerState<_SectionFieldsDialog> {
  late final TextEditingController title;
  late final TextEditingController body;
  late final TextEditingController target;
  late String kind;
  late String format;
  bool running = false;
  String? error;

  static const kinds = [
    'text',
    'authors',
    'journal',
    'project',
    'series',
    'code',
    'model',
  ];

  @override
  void initState() {
    super.initState();
    title = TextEditingController(text: widget.section?.title ?? '');
    body = TextEditingController(text: widget.section?.body ?? '');
    target = TextEditingController();
    kind = widget.section?.kind ?? 'text';
    format = widget.section?.format ?? 'markdown';
  }

  @override
  void dispose() {
    title.dispose();
    body.dispose();
    target.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    if (!ref.read(unlockedProvider)) {
      setState(() => error = 'Unlock the store first.');
      return;
    }
    setState(() {
      running = true;
      error = null;
    });
    try {
      final api = ref.read(apiProvider);
      final ReportSectionInfo result;
      if (widget.section == null) {
        result = await api.addReportSection(
          widget.report.id,
          kind: kind,
          title: title.text.trim(),
          body: body.text,
          format: format,
          target: target.text.trim().isEmpty ? null : target.text.trim(),
        );
      } else {
        result = await api.updateReportSection(
          widget.report.id,
          widget.section!.id,
          title: title.text.trim(),
          body: body.text,
          format: format,
        );
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
    final create = widget.section == null;
    final needsTarget =
        kind == 'journal' ||
        kind == 'project' ||
        kind == 'series' ||
        kind == 'code' ||
        kind == 'model';
    return AlertDialog(
      title: Row(
        children: [
          Expanded(
            child: Text(
              create ? 'Add section' : 'Edit section',
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
        width: 480,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            if (create) ...[
              DropdownButtonFormField<String>(
                key: const ValueKey('section-kind'),
                initialValue: kind,
                decoration: const InputDecoration(labelText: 'Kind'),
                items: [
                  for (final k in kinds)
                    DropdownMenuItem(value: k, child: Text(k)),
                ],
                onChanged: (v) {
                  if (v != null) setState(() => kind = v);
                },
              ),
              const SizedBox(height: 12),
            ],
            TextField(
              key: const ValueKey('section-title'),
              controller: title,
              decoration: const InputDecoration(labelText: 'Title'),
            ),
            const SizedBox(height: 12),
            TextField(
              key: const ValueKey('section-body'),
              controller: body,
              minLines: 3,
              maxLines: 6,
              decoration: const InputDecoration(labelText: 'Body'),
            ),
            if (create && needsTarget) ...[
              const SizedBox(height: 12),
              TextField(
                key: const ValueKey('section-target'),
                controller: target,
                decoration: const InputDecoration(
                  labelText: 'Target (name or id)',
                ),
              ),
            ],
            const SizedBox(height: 12),
            DropdownButtonFormField<String>(
              initialValue: format,
              decoration: const InputDecoration(labelText: 'Format'),
              items: const [
                DropdownMenuItem(value: 'markdown', child: Text('markdown')),
                DropdownMenuItem(value: 'latex', child: Text('latex')),
              ],
              onChanged: (v) {
                if (v != null) setState(() => format = v);
              },
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
          key: const ValueKey('section-save'),
          onPressed: running ? null : _save,
          child: const Text('Save'),
        ),
      ],
    );
  }
}

class _ImportProjectDialog extends ConsumerStatefulWidget {
  const _ImportProjectDialog();

  @override
  ConsumerState<_ImportProjectDialog> createState() =>
      _ImportProjectDialogState();
}

class _ImportProjectDialogState extends ConsumerState<_ImportProjectDialog> {
  final path = TextEditingController();
  final name = TextEditingController();
  bool running = false;
  String? error;

  @override
  void dispose() {
    path.dispose();
    name.dispose();
    super.dispose();
  }

  Future<void> _browse() async {
    try {
      final picked = (await FilePicker.platform.pickFiles(
        type: FileType.custom,
        allowedExtensions: const ['mnnp', 'json'],
      ))?.files.single.path;
      if (picked != null) setState(() => path.text = picked);
    } catch (e) {
      if (mounted) {
        setState(
          () => error = 'Could not open the picker. Type a path instead.',
        );
      }
    }
  }

  Future<void> _save() async {
    final pth = path.text.trim();
    if (pth.isEmpty) {
      setState(() => error = 'Choose a file.');
      return;
    }
    if (!ref.read(unlockedProvider)) {
      setState(() => error = 'Unlock the store first.');
      return;
    }
    setState(() {
      running = true;
      error = null;
    });
    try {
      final msg = await ref
          .read(apiProvider)
          .projectImport(
            pth,
            name: name.text.trim().isEmpty ? null : name.text.trim(),
          );
      if (mounted) Navigator.pop(context, msg);
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
              'Import project archive',
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
        width: 480,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            TextField(
              key: const ValueKey('project-import-path'),
              controller: path,
              decoration: InputDecoration(
                labelText: 'Archive path (.mnnp or JSON)',
                suffixIcon: IconButton(
                  tooltip: 'Browse',
                  onPressed: _browse,
                  icon: const Icon(Icons.folder_open),
                ),
              ),
            ),
            const SizedBox(height: 12),
            TextField(
              key: const ValueKey('project-import-name'),
              controller: name,
              decoration: const InputDecoration(labelText: 'Name (optional)'),
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
          key: const ValueKey('project-import-save'),
          onPressed: running ? null : _save,
          child: const Text('Import'),
        ),
      ],
    );
  }
}

class _ExportProjectDialog extends ConsumerStatefulWidget {
  const _ExportProjectDialog({required this.project});
  final Project project;

  @override
  ConsumerState<_ExportProjectDialog> createState() =>
      _ExportProjectDialogState();
}

class _ExportProjectDialogState extends ConsumerState<_ExportProjectDialog> {
  final path = TextEditingController();
  String format = 'archive';
  bool noData = false;
  bool running = false;
  String? error;

  @override
  void dispose() {
    path.dispose();
    super.dispose();
  }

  Future<void> _browse() async {
    try {
      final ext = format == 'json' ? 'json' : 'mnnp';
      final picked = await FilePicker.platform.saveFile(
        fileName: '${widget.project.name}.$ext',
        type: FileType.custom,
        allowedExtensions: [ext],
      );
      if (picked != null) setState(() => path.text = picked);
    } catch (e) {
      if (mounted) {
        setState(
          () => error = 'Could not open the picker. Type a path instead.',
        );
      }
    }
  }

  Future<void> _save() async {
    final pth = path.text.trim();
    if (pth.isEmpty) {
      setState(() => error = 'Choose a file.');
      return;
    }
    if (!ref.read(unlockedProvider)) {
      setState(() => error = 'Unlock the store first.');
      return;
    }
    setState(() {
      running = true;
      error = null;
    });
    try {
      final msg = await ref
          .read(apiProvider)
          .projectExport(
            widget.project.id,
            pth,
            format: format,
            noData: noData,
          );
      if (mounted) Navigator.pop(context, msg);
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
              'Export ${widget.project.name}',
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
        width: 480,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            TextField(
              key: const ValueKey('project-export-path'),
              controller: path,
              decoration: InputDecoration(
                labelText: 'Output path',
                suffixIcon: IconButton(
                  tooltip: 'Browse',
                  onPressed: _browse,
                  icon: const Icon(Icons.folder_open),
                ),
              ),
            ),
            const SizedBox(height: 12),
            DropdownButtonFormField<String>(
              key: const ValueKey('project-export-format'),
              initialValue: format,
              decoration: const InputDecoration(labelText: 'Format'),
              items: const [
                DropdownMenuItem(
                  value: 'archive',
                  child: Text('archive (.mnnp)'),
                ),
                DropdownMenuItem(value: 'json', child: Text('json')),
              ],
              onChanged: (v) {
                if (v != null) setState(() => format = v);
              },
            ),
            CheckboxListTile(
              key: const ValueKey('project-export-nodata'),
              contentPadding: EdgeInsets.zero,
              title: const Text('No series data'),
              value: noData,
              onChanged: (v) => setState(() => noData = v ?? false),
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
          key: const ValueKey('project-export-save'),
          onPressed: running ? null : _save,
          child: const Text('Export'),
        ),
      ],
    );
  }
}

class _AddProjectItemDialog extends ConsumerStatefulWidget {
  const _AddProjectItemDialog({required this.project});
  final Project project;

  @override
  ConsumerState<_AddProjectItemDialog> createState() =>
      _AddProjectItemDialogState();
}

class _AddProjectItemDialogState extends ConsumerState<_AddProjectItemDialog> {
  final keyCtl = TextEditingController();
  String kind = 'journal';
  bool running = false;
  String? error;

  static const kinds = ['journal', 'search', 'report', 'rule', 'model', 'code'];

  @override
  void dispose() {
    keyCtl.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final key = keyCtl.text.trim();
    if (key.isEmpty) {
      setState(() => error = 'Name or id is required.');
      return;
    }
    if (!ref.read(unlockedProvider)) {
      setState(() => error = 'Unlock the store first.');
      return;
    }
    setState(() {
      running = true;
      error = null;
    });
    try {
      final msg = await ref
          .read(apiProvider)
          .addProjectItem(widget.project.id, kind, key);
      if (mounted) Navigator.pop(context, msg);
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
              'Add to ${widget.project.name}',
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
            DropdownButtonFormField<String>(
              key: const ValueKey('project-add-kind'),
              initialValue: kind,
              decoration: const InputDecoration(labelText: 'Kind'),
              items: [
                for (final k in kinds)
                  DropdownMenuItem(value: k, child: Text(k)),
              ],
              onChanged: (v) {
                if (v != null) setState(() => kind = v);
              },
            ),
            const SizedBox(height: 12),
            TextField(
              key: const ValueKey('project-add-key'),
              controller: keyCtl,
              decoration: const InputDecoration(labelText: 'Name or id'),
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
          key: const ValueKey('project-add-save'),
          onPressed: running ? null : _save,
          child: const Text('Add'),
        ),
      ],
    );
  }
}

class _ConfirmRemoveDialog extends ConsumerStatefulWidget {
  const _ConfirmRemoveDialog({
    required this.title,
    required this.body,
    required this.confirmKey,
    required this.run,
  });
  final String title, body, confirmKey;
  final Future<String> Function() run;

  @override
  ConsumerState<_ConfirmRemoveDialog> createState() =>
      _ConfirmRemoveDialogState();
}

class _ConfirmRemoveDialogState extends ConsumerState<_ConfirmRemoveDialog> {
  bool running = false;
  String? error;

  Future<void> _go() async {
    setState(() {
      running = true;
      error = null;
    });
    try {
      await widget.run();
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
              widget.title,
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
          Text(widget.body),
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
          key: ValueKey(widget.confirmKey),
          onPressed: running ? null : _go,
          style: FilledButton.styleFrom(backgroundColor: m.error),
          child: const Text('Delete'),
        ),
      ],
    );
  }
}
