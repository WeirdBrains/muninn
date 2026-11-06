/// Journal leftovers: word-span highlights, images, copy entries, document
/// export/import and print (`mnnn journal highlight|attach|copy|export|import|print`).
/// Themes via [context.muninn]. Image bytes stay in Rust; Dart only passes paths.
library;

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../state.dart';
import '../theme/muninn_theme.dart';

void _ok(BuildContext context, String msg, {Color? color}) {
  if (!context.mounted) {
    return;
  }
  ScaffoldMessenger.of(context)
      .showSnackBar(SnackBar(content: Text(msg), backgroundColor: color));
}

Future<String?> _pickOpen() async {
  final r = await FilePicker.platform.pickFiles();
  return r?.files.single.path;
}

Future<String?> _pickSave(String name) =>
    FilePicker.platform.saveFile(fileName: name);

Future<void> showJournalImportDialog(
  BuildContext context,
  WidgetRef ref,
) async {
  ref.read(sectionProvider.notifier).state = Section.projects;
  ref.read(projectPaneProvider.notifier).state = ProjectPane.journals;
  final imported = await showDialog<JournalInfo>(
    context: context,
    builder: (ctx) => const _ImportJournalDialog(),
  );
  if (imported == null) {
    return;
  }
  refreshProjectExtras(ref);
  ref.read(selectedJournalProvider.notifier).state = imported;
}

Future<void> showJournalExportDialog(
  BuildContext context,
  WidgetRef ref,
  JournalInfo journal,
) async {
  final ok = await showDialog<JournalExportInfo>(
    context: context,
    builder: (ctx) => _ExportJournalDialog(journal: journal),
  );
  if (ok == null || !context.mounted) {
    return;
  }
  refreshProjectExtras(ref);
  _ok(context, ok.message.isEmpty ? 'exported to ${ok.path}' : ok.message);
}

Future<void> showJournalPrintDialog(
  BuildContext context,
  WidgetRef ref,
  JournalInfo journal,
) async {
  final ok = await showDialog<JournalExportInfo>(
    context: context,
    builder: (ctx) => _PrintJournalDialog(journal: journal),
  );
  if (ok == null || !context.mounted) {
    return;
  }
  _ok(context, ok.message.isEmpty ? 'saved ${ok.path}' : ok.message);
}

Future<void> showJournalHighlightDialog(
  BuildContext context,
  WidgetRef ref,
  JournalEntryInfo entry,
) async {
  final created = await showDialog<JournalAnnotationInfo>(
    context: context,
    builder: (ctx) => _HighlightDialog(entry: entry),
  );
  if (created == null) {
    return;
  }
  refreshProjectExtras(ref);
}

Future<void> showJournalEntryNoteDialog(
  BuildContext context,
  WidgetRef ref,
  JournalEntryInfo entry,
) async {
  final created = await showDialog<JournalAnnotationInfo>(
    context: context,
    builder: (ctx) => _EntryNoteDialog(entry: entry),
  );
  if (created == null) {
    return;
  }
  refreshProjectExtras(ref);
}

Future<void> showJournalAttachDialog(
  BuildContext context,
  WidgetRef ref,
  JournalEntryInfo entry,
) async {
  final created = await showDialog<JournalImageInfo>(
    context: context,
    builder: (ctx) => _AttachImageDialog(entry: entry),
  );
  if (created == null) {
    return;
  }
  refreshProjectExtras(ref);
}

Future<void> showJournalCopyDialog(
  BuildContext context,
  WidgetRef ref,
  List<String> entries,
) async {
  final copied = await showDialog<List<JournalEntryInfo>>(
    context: context,
    builder: (ctx) => _CopyEntriesDialog(entries: entries),
  );
  if (copied == null) {
    return;
  }
  refreshProjectExtras(ref);
}

/// Highlight, attach, copy and word-notes for one entry, plus its marks and images.
class JournalEntryExtras extends ConsumerWidget {
  const JournalEntryExtras(this.entry, {super.key});
  final JournalEntryInfo entry;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final api = ref.watch(apiProvider);
    return Padding(
      padding: const EdgeInsets.fromLTRB(72, 0, 16, 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              OutlinedButton(
                key: ValueKey('entry-highlight-${entry.id}'),
                onPressed: () =>
                    showJournalHighlightDialog(context, ref, entry),
                child: const Text('Highlight'),
              ),
              OutlinedButton(
                key: ValueKey('entry-note-words-${entry.id}'),
                onPressed: () =>
                    showJournalEntryNoteDialog(context, ref, entry),
                child: const Text('Note on words'),
              ),
              OutlinedButton(
                key: ValueKey('entry-attach-${entry.id}'),
                onPressed: () => showJournalAttachDialog(context, ref, entry),
                child: const Text('Attach image'),
              ),
              OutlinedButton(
                key: ValueKey('entry-copy-${entry.id}'),
                onPressed: () =>
                    showJournalCopyDialog(context, ref, [entry.id]),
                child: const Text('Copy'),
              ),
            ],
          ),
          FutureBuilder<List<JournalAnnotationInfo>>(
            future: api.entryAnnotations(entry.id),
            builder: (context, snap) {
              final anns = snap.data ?? const <JournalAnnotationInfo>[];
              if (anns.isEmpty) {
                return const SizedBox.shrink();
              }
              return Column(
                children: [
                  for (final a in anns)
                    ListTile(
                      dense: true,
                      contentPadding: EdgeInsets.zero,
                      leading: Icon(
                        a.kind == 'highlight'
                            ? Icons.highlight
                            : Icons.sticky_note_2_outlined,
                        color: a.kind == 'highlight' ? m.warning : m.info,
                      ),
                      title: Text(
                        a.kind == 'highlight'
                            ? 'highlight on "${a.quote ?? ''}"'
                            : a.quote == null || a.quote!.isEmpty
                            ? 'note on the entry'
                            : 'note on "${a.quote}"',
                      ),
                      subtitle: a.body.isEmpty && !a.detached
                          ? null
                          : Text(
                              [
                                if (a.body.isNotEmpty) a.body,
                                if (a.detached) 'words no longer in the entry',
                              ].join(' · '),
                              style: TextStyle(color: m.muted),
                            ),
                      trailing: IconButton(
                        key: ValueKey('ann-remove-${a.id}'),
                        tooltip: 'Remove',
                        icon: Icon(Icons.remove_circle_outline, color: m.muted),
                        onPressed: () async {
                          try {
                            await ref
                                .read(apiProvider)
                                .removeJournalAnnotation(a.id);
                            refreshProjectExtras(ref);
                          } catch (e) {
                            if (context.mounted) {
                              _ok(context, '$e', color: m.error);
                            }
                          }
                        },
                      ),
                    ),
                ],
              );
            },
          ),
          FutureBuilder<List<JournalImageInfo>>(
            future: api.entryImages(entry.id),
            builder: (context, snap) {
              final imgs = snap.data ?? const <JournalImageInfo>[];
              if (imgs.isEmpty) {
                return const SizedBox.shrink();
              }
              return Column(
                children: [
                  for (final img in imgs)
                    ListTile(
                      dense: true,
                      contentPadding: EdgeInsets.zero,
                      leading: Icon(Icons.image_outlined, color: m.accent),
                      title: Text(img.name),
                      subtitle: Text(
                        '${img.mime} · ${img.size} bytes',
                        style: TextStyle(color: m.muted),
                      ),
                      trailing: IconButton(
                        key: ValueKey('image-save-${img.id}'),
                        tooltip: 'Save image',
                        icon: const Icon(Icons.save_alt),
                        onPressed: () async {
                          final out = await showDialog<String>(
                            context: context,
                            builder: (ctx) => _SaveImageDialog(image: img),
                          );
                          if (out == null || !context.mounted) {
                            return;
                          }
                          try {
                            final msg = await ref
                                .read(apiProvider)
                                .saveJournalImage(img.id, out);
                            if (context.mounted) {
                              _ok(context, msg);
                            }
                          } catch (e) {
                            if (context.mounted) {
                              _ok(context, '$e', color: m.error);
                            }
                          }
                        },
                      ),
                    ),
                ],
              );
            },
          ),
        ],
      ),
    );
  }
}

class _HighlightDialog extends ConsumerStatefulWidget {
  const _HighlightDialog({required this.entry});
  final JournalEntryInfo entry;

  @override
  ConsumerState<_HighlightDialog> createState() => _HighlightDialogState();
}

class _HighlightDialogState extends ConsumerState<_HighlightDialog> {
  final words = TextEditingController();
  final comment = TextEditingController();
  bool running = false;
  String? error;

  @override
  void dispose() {
    words.dispose();
    comment.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final w = words.text.trim();
    if (w.isEmpty) {
      setState(() => error = 'Select words that appear in the entry.');
      return;
    }
    setState(() {
      running = true;
      error = null;
    });
    try {
      final a = await ref
          .read(apiProvider)
          .addEntryHighlight(
            widget.entry.id,
            words: w,
            comment: comment.text.trim().isEmpty ? null : comment.text.trim(),
          );
      if (mounted) {
        Navigator.pop(context, a);
      }
    } catch (e) {
      if (mounted) {
        setState(() => error = e.toString());
      }
    } finally {
      if (mounted) {
        setState(() => running = false);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      title: Text(
        'Highlight words',
        style: TextStyle(color: m.header, fontWeight: FontWeight.w800),
      ),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              widget.entry.heading.isEmpty
                  ? widget.entry.title
                  : widget.entry.heading,
              style: TextStyle(color: m.muted),
            ),
            TextField(
              key: const ValueKey('journal-hl-words'),
              controller: words,
              autofocus: true,
              decoration: const InputDecoration(
                labelText: 'Words (first occurrence)',
              ),
            ),
            TextField(
              key: const ValueKey('journal-hl-comment'),
              controller: comment,
              decoration: const InputDecoration(
                labelText: 'Comment (optional)',
              ),
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
          key: const ValueKey('journal-hl-save'),
          onPressed: running ? null : _save,
          child: const Text('Highlight'),
        ),
      ],
    );
  }
}

class _EntryNoteDialog extends ConsumerStatefulWidget {
  const _EntryNoteDialog({required this.entry});
  final JournalEntryInfo entry;

  @override
  ConsumerState<_EntryNoteDialog> createState() => _EntryNoteDialogState();
}

class _EntryNoteDialogState extends ConsumerState<_EntryNoteDialog> {
  final body = TextEditingController();
  final words = TextEditingController();
  bool running = false;
  String? error;

  @override
  void dispose() {
    body.dispose();
    words.dispose();
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
      final a = await ref
          .read(apiProvider)
          .addEntryNote(
            widget.entry.id,
            text,
            words: words.text.trim().isEmpty ? null : words.text.trim(),
          );
      if (mounted) {
        Navigator.pop(context, a);
      }
    } catch (e) {
      if (mounted) {
        setState(() => error = e.toString());
      }
    } finally {
      if (mounted) {
        setState(() => running = false);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      title: Text(
        'Note on entry',
        style: TextStyle(color: m.header, fontWeight: FontWeight.w800),
      ),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              key: const ValueKey('journal-entry-note-body'),
              controller: body,
              autofocus: true,
              minLines: 2,
              maxLines: 4,
              decoration: const InputDecoration(labelText: 'Note'),
            ),
            TextField(
              key: const ValueKey('journal-entry-note-words'),
              controller: words,
              decoration: const InputDecoration(
                labelText: 'Words (optional; empty notes the whole entry)',
              ),
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
          key: const ValueKey('journal-entry-note-save'),
          onPressed: running ? null : _save,
          child: const Text('Save'),
        ),
      ],
    );
  }
}

class _AttachImageDialog extends ConsumerStatefulWidget {
  const _AttachImageDialog({required this.entry});
  final JournalEntryInfo entry;

  @override
  ConsumerState<_AttachImageDialog> createState() => _AttachImageDialogState();
}

class _AttachImageDialogState extends ConsumerState<_AttachImageDialog> {
  final path = TextEditingController();
  bool running = false;
  String? error;

  @override
  void dispose() {
    path.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final p = path.text.trim();
    if (p.isEmpty) {
      setState(() => error = 'Give the image file path.');
      return;
    }
    setState(() {
      running = true;
      error = null;
    });
    try {
      final img = await ref
          .read(apiProvider)
          .attachJournalImage(widget.entry.id, p);
      if (mounted) {
        Navigator.pop(context, img);
      }
    } catch (e) {
      if (mounted) {
        setState(() => error = e.toString());
      }
    } finally {
      if (mounted) {
        setState(() => running = false);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      title: Text(
        'Attach image',
        style: TextStyle(color: m.header, fontWeight: FontWeight.w800),
      ),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              'Rust reads the file. The image bytes stay in the store.',
              style: TextStyle(color: m.muted),
            ),
            TextField(
              key: const ValueKey('journal-attach-path'),
              controller: path,
              autofocus: true,
              decoration: const InputDecoration(labelText: 'Image path'),
            ),
            Align(
              alignment: Alignment.centerLeft,
              child: TextButton(
                onPressed: () async {
                  final picked = await _pickOpen();
                  if (picked != null) {
                    path.text = picked;
                  }
                },
                child: const Text('Browse…'),
              ),
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
          key: const ValueKey('journal-attach-save'),
          onPressed: running ? null : _save,
          child: const Text('Attach'),
        ),
      ],
    );
  }
}

class _CopyEntriesDialog extends ConsumerStatefulWidget {
  const _CopyEntriesDialog({required this.entries});
  final List<String> entries;

  @override
  ConsumerState<_CopyEntriesDialog> createState() => _CopyEntriesDialogState();
}

class _CopyEntriesDialogState extends ConsumerState<_CopyEntriesDialog> {
  final to = TextEditingController();
  final newName = TextEditingController();
  bool running = false;
  String? error;

  @override
  void dispose() {
    to.dispose();
    newName.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final target = to.text.trim();
    final created = newName.text.trim();
    if (target.isEmpty && created.isEmpty) {
      setState(() => error = 'Copy to an existing journal or name a new one.');
      return;
    }
    setState(() {
      running = true;
      error = null;
    });
    try {
      final copied = await ref
          .read(apiProvider)
          .copyJournalEntries(
            widget.entries,
            to: target.isEmpty ? null : target,
            newName: created.isEmpty ? null : created,
          );
      if (mounted) {
        Navigator.pop(context, copied);
      }
    } catch (e) {
      if (mounted) {
        setState(() => error = e.toString());
      }
    } finally {
      if (mounted) {
        setState(() => running = false);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      title: Text(
        'Copy entries',
        style: TextStyle(color: m.header, fontWeight: FontWeight.w800),
      ),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              key: const ValueKey('journal-copy-to'),
              controller: to,
              decoration: const InputDecoration(
                labelText: 'Existing journal (id or name)',
              ),
            ),
            TextField(
              key: const ValueKey('journal-copy-new'),
              controller: newName,
              decoration: const InputDecoration(
                labelText: 'Or new journal name',
              ),
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
          key: const ValueKey('journal-copy-save'),
          onPressed: running ? null : _save,
          child: const Text('Copy'),
        ),
      ],
    );
  }
}

class _ExportJournalDialog extends ConsumerStatefulWidget {
  const _ExportJournalDialog({required this.journal});
  final JournalInfo journal;

  @override
  ConsumerState<_ExportJournalDialog> createState() =>
      _ExportJournalDialogState();
}

class _ExportJournalDialogState extends ConsumerState<_ExportJournalDialog> {
  final out = TextEditingController();
  String format = 'pdf';
  bool running = false;
  String? error;

  @override
  void dispose() {
    out.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final dest = out.text.trim();
    if (dest.isEmpty) {
      setState(() => error = 'Give a path to write.');
      return;
    }
    setState(() {
      running = true;
      error = null;
    });
    try {
      final r = await ref
          .read(apiProvider)
          .exportJournal(widget.journal.id, format: format, out: dest);
      if (mounted) {
        Navigator.pop(context, r);
      }
    } catch (e) {
      if (mounted) {
        setState(() => error = e.toString());
      }
    } finally {
      if (mounted) {
        setState(() => running = false);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      title: Text(
        'Export ${widget.journal.name}',
        style: TextStyle(color: m.header, fontWeight: FontWeight.w800),
      ),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            DropdownButtonFormField<String>(
              key: const ValueKey('journal-export-format'),
              initialValue: format,
              decoration: const InputDecoration(labelText: 'Format'),
              items: const [
                DropdownMenuItem(value: 'pdf', child: Text('PDF')),
                DropdownMenuItem(value: 'docx', child: Text('Word')),
                DropdownMenuItem(value: 'html', child: Text('HTML')),
                DropdownMenuItem(value: 'latex', child: Text('LaTeX')),
                DropdownMenuItem(value: 'text', child: Text('Plain text')),
                DropdownMenuItem(value: 'json', child: Text('JSON')),
              ],
              onChanged: (v) {
                if (v != null) {
                  setState(() => format = v);
                }
              },
            ),
            TextField(
              key: const ValueKey('journal-export-out'),
              controller: out,
              decoration: const InputDecoration(labelText: 'Output path'),
            ),
            Align(
              alignment: Alignment.centerLeft,
              child: TextButton(
                onPressed: () async {
                  final picked = await _pickSave(
                    '${widget.journal.name}.$format',
                  );
                  if (picked != null) {
                    out.text = picked;
                  }
                },
                child: const Text('Browse…'),
              ),
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
          key: const ValueKey('journal-export-save'),
          onPressed: running ? null : _save,
          child: const Text('Export'),
        ),
      ],
    );
  }
}

class _PrintJournalDialog extends ConsumerStatefulWidget {
  const _PrintJournalDialog({required this.journal});
  final JournalInfo journal;

  @override
  ConsumerState<_PrintJournalDialog> createState() =>
      _PrintJournalDialogState();
}

class _PrintJournalDialogState extends ConsumerState<_PrintJournalDialog> {
  final out = TextEditingController();
  final printer = TextEditingController();
  bool running = false;
  String? error;

  @override
  void dispose() {
    out.dispose();
    printer.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    setState(() {
      running = true;
      error = null;
    });
    try {
      final r = await ref
          .read(apiProvider)
          .printJournal(
            widget.journal.id,
            out: out.text.trim().isEmpty ? null : out.text.trim(),
            printer: printer.text.trim().isEmpty ? null : printer.text.trim(),
            saveOnly: printer.text.trim().isEmpty,
          );
      if (mounted) {
        Navigator.pop(context, r);
      }
    } catch (e) {
      if (mounted) {
        setState(() => error = e.toString());
      }
    } finally {
      if (mounted) {
        setState(() => running = false);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      title: Text(
        'Print ${widget.journal.name}',
        style: TextStyle(color: m.header, fontWeight: FontWeight.w800),
      ),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              'Saves a PDF. Leave the printer empty to keep the file only.',
              style: TextStyle(color: m.muted),
            ),
            TextField(
              key: const ValueKey('journal-print-out'),
              controller: out,
              decoration: const InputDecoration(
                labelText: 'PDF path (optional)',
              ),
            ),
            TextField(
              key: const ValueKey('journal-print-printer'),
              controller: printer,
              decoration: const InputDecoration(
                labelText: 'Printer (optional)',
              ),
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
          key: const ValueKey('journal-print-save'),
          onPressed: running ? null : _save,
          child: const Text('Save PDF'),
        ),
      ],
    );
  }
}

class _ImportJournalDialog extends ConsumerStatefulWidget {
  const _ImportJournalDialog();

  @override
  ConsumerState<_ImportJournalDialog> createState() =>
      _ImportJournalDialogState();
}

class _ImportJournalDialogState extends ConsumerState<_ImportJournalDialog> {
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

  Future<void> _save() async {
    final p = path.text.trim();
    if (p.isEmpty) {
      setState(() => error = 'Give the exported JSON path.');
      return;
    }
    setState(() {
      running = true;
      error = null;
    });
    try {
      final j = await ref
          .read(apiProvider)
          .importJournal(
            p,
            name: name.text.trim().isEmpty ? null : name.text.trim(),
          );
      if (mounted) {
        Navigator.pop(context, j);
      }
    } catch (e) {
      if (mounted) {
        setState(() => error = e.toString());
      }
    } finally {
      if (mounted) {
        setState(() => running = false);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      title: Text(
        'Import journal',
        style: TextStyle(color: m.header, fontWeight: FontWeight.w800),
      ),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              key: const ValueKey('journal-import-path'),
              controller: path,
              autofocus: true,
              decoration: const InputDecoration(
                labelText: 'Exported JSON path',
              ),
            ),
            Align(
              alignment: Alignment.centerLeft,
              child: TextButton(
                onPressed: () async {
                  final picked = await _pickOpen();
                  if (picked != null) {
                    path.text = picked;
                  }
                },
                child: const Text('Browse…'),
              ),
            ),
            TextField(
              key: const ValueKey('journal-import-name'),
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
          key: const ValueKey('journal-import-save'),
          onPressed: running ? null : _save,
          child: const Text('Import'),
        ),
      ],
    );
  }
}

class _SaveImageDialog extends StatefulWidget {
  const _SaveImageDialog({required this.image});
  final JournalImageInfo image;

  @override
  State<_SaveImageDialog> createState() => _SaveImageDialogState();
}

class _SaveImageDialogState extends State<_SaveImageDialog> {
  late final TextEditingController out = TextEditingController(
    text: widget.image.name,
  );

  @override
  void dispose() {
    out.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      title: Text(
        'Save ${widget.image.name}',
        style: TextStyle(color: m.header, fontWeight: FontWeight.w800),
      ),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              key: const ValueKey('journal-image-out'),
              controller: out,
              autofocus: true,
              decoration: const InputDecoration(labelText: 'Output path'),
            ),
            Align(
              alignment: Alignment.centerLeft,
              child: TextButton(
                onPressed: () async {
                  final picked = await _pickSave(widget.image.name);
                  if (picked != null) {
                    out.text = picked;
                  }
                },
                child: const Text('Browse…'),
              ),
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
          key: const ValueKey('journal-image-save'),
          onPressed: () => Navigator.pop(context, out.text.trim()),
          child: const Text('Save'),
        ),
      ],
    );
  }
}
