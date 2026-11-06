/// File ▸ Import… and Export… : store files as they are, or write bars (CSV/Parquet/JSON).
library;

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'api.dart';
import 'state.dart';
import 'theme/muninn_theme.dart';
import 'time_range.dart';

Future<void> showImportDialog(BuildContext context, WidgetRef ref) => showDialog(
      context: context,
      builder: (ctx) => _ImportDialog(api: ref.read(apiProvider), unlocked: ref.read(unlockedProvider)),
    ).then((changed) {
      if (changed == true) ref.invalidate(overviewProvider);
    });

Future<void> showExportDialog(BuildContext context, WidgetRef ref) {
  final d = ref.read(selectedDatasetValueProvider);
  final r = ref.read(timeRangeProvider);
  return showDialog(
    context: context,
    builder: (ctx) => _ExportDialog(
      api: ref.read(apiProvider),
      unlocked: ref.read(unlockedProvider),
      source: d?.source,
      symbol: d?.symbol,
      duration: d?.duration,
      from: r.from,
      to: r.to,
    ),
  );
}

class _ImportDialog extends StatefulWidget {
  const _ImportDialog({required this.api, required this.unlocked});
  final MuninnApi api;
  final bool unlocked;
  @override
  State<_ImportDialog> createState() => _ImportDialogState();
}

class _ImportDialogState extends State<_ImportDialog> {
  final path = TextEditingController();
  final source = TextEditingController();
  bool running = false;
  IngestProgress? progress;
  IngestResult? result;
  String? error;

  @override
  void dispose() {
    path.dispose();
    source.dispose();
    super.dispose();
  }

  Future<void> _browse({required bool directory}) async {
    try {
      final picked = directory
          ? await FilePicker.platform.getDirectoryPath()
          : (await FilePicker.platform.pickFiles(
              type: FileType.custom,
              allowedExtensions: const ['csv', 'tsv', 'txt', 'json', 'ndjson', 'jsonl'],
            ))
              ?.files
              .single
              .path;
      if (picked != null) setState(() => path.text = picked);
    } catch (e) {
      if (mounted) setState(() => error = 'Could not open the picker. Type a path instead.');
    }
  }

  Future<void> _start() async {
    final p = path.text.trim();
    if (p.isEmpty) {
      setState(() => error = 'Choose a file or folder.');
      return;
    }
    if (!widget.unlocked) {
      setState(() => error = 'Unlock the store first.');
      return;
    }
    setState(() {
      running = true;
      error = null;
      result = null;
      progress = null;
    });
    final tick = Stream.periodic(const Duration(milliseconds: 100), (_) => widget.api.ingestProgress());
    final sub = tick.listen((p) {
      if (mounted) setState(() => progress = p);
    });
    try {
      final r = await widget.api.ingestPath(p, source: source.text.trim().isEmpty ? null : source.text.trim());
      if (mounted) setState(() => result = r);
    } catch (e) {
      if (mounted) setState(() => error = e.toString());
    } finally {
      await sub.cancel();
      if (mounted) setState(() => running = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    final title = Text('Import', style: TextStyle(color: m.header, fontWeight: FontWeight.w800));
    return AlertDialog(
      title: Row(children: [
        Expanded(child: title),
        IconButton(tooltip: 'Close', icon: const Text('[X]'), onPressed: running ? null : () => Navigator.pop(context, result != null)),
      ]),
      content: SizedBox(
        width: 520,
        child: result != null
            ? _importSummary(m, result!)
            : Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                Text(
                  'Stores each file as it is in a source. No bars are made — turn stored files into bars with a pipe or pipeline.',
                  style: TextStyle(color: m.muted),
                ),
                const SizedBox(height: 12),
                TextField(
                  key: const ValueKey('import-path'),
                  controller: path,
                  enabled: !running,
                  decoration: const InputDecoration(labelText: 'File or folder', hintText: '/data/binance/'),
                ),
                const SizedBox(height: 8),
                Wrap(spacing: 8, children: [
                  OutlinedButton(onPressed: running ? null : () => _browse(directory: false), child: const Text('Browse file')),
                  OutlinedButton(onPressed: running ? null : () => _browse(directory: true), child: const Text('Browse folder')),
                ]),
                const SizedBox(height: 8),
                TextField(
                  key: const ValueKey('import-source'),
                  controller: source,
                  enabled: !running,
                  decoration: const InputDecoration(labelText: 'Source (optional)', hintText: 'folder name, or local'),
                ),
                if (!widget.unlocked) ...[
                  const SizedBox(height: 12),
                  Text('Unlock the store first (Home).', style: TextStyle(color: m.warning)),
                ],
                if (running) ...[
                  const SizedBox(height: 16),
                  LinearProgressIndicator(value: progress == null || progress!.bytesTotal == 0 ? null : progress!.overall),
                  const SizedBox(height: 8),
                  Text(
                    progress == null
                        ? 'Starting…'
                        : 'file ${progress!.fileIndex + 1}  ·  ${progress!.bytesDone} / ${progress!.bytesTotal} bytes',
                    style: TextStyle(color: m.muted),
                  ),
                ],
                if (error != null) ...[
                  const SizedBox(height: 12),
                  Text(error!, style: TextStyle(color: m.error)),
                ],
              ]),
      ),
      actions: [
        if (running)
          TextButton(
            key: const ValueKey('import-cancel'),
            onPressed: () => widget.api.cancelIngest(),
            child: const Text('Cancel'),
          )
        else if (result != null)
          TextButton(onPressed: () => Navigator.pop(context, true), child: const Text('Close'))
        else ...[
          TextButton(onPressed: () => Navigator.pop(context, false), child: const Text('Close')),
          FilledButton(key: const ValueKey('import-start'), onPressed: _start, child: const Text('Import')),
        ],
      ],
    );
  }

  Widget _importSummary(MuninnColors m, IngestResult r) {
    if (r.cancelled) {
      return Text(
        'Import cancelled. Files that finished were kept; the file in progress was rolled back.',
        style: TextStyle(color: m.warning),
      );
    }
    final stored = r.files.where((f) => !f.skipped).length;
    final skipped = r.files.where((f) => f.skipped).length;
    return Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
      Text('Stored $stored file${stored == 1 ? '' : 's'}'
          '${skipped == 0 ? '' : ', skipped $skipped already in the source'}.', style: TextStyle(color: m.success)),
      const SizedBox(height: 8),
      Text('Read them later with a pipeline, e.g. source:NAME → code:parse-csv → code:to-bars → model:bars.', style: TextStyle(color: m.muted)),
      const SizedBox(height: 8),
      ConstrainedBox(
        constraints: const BoxConstraints(maxHeight: 240),
        child: ListView(
          shrinkWrap: true,
          children: [
            for (final f in r.files)
              ListTile(
                dense: true,
                title: Text(f.locator, maxLines: 2, overflow: TextOverflow.ellipsis),
                subtitle: Text('${f.source} · ${f.format.isEmpty ? 'data' : f.format} · ${f.bytes} bytes${f.skipped ? ' · skipped' : ''}'),
              ),
          ],
        ),
      ),
    ]);
  }
}

class _ExportDialog extends StatefulWidget {
  const _ExportDialog({
    required this.api,
    required this.unlocked,
    this.source,
    this.symbol,
    this.duration,
    this.from,
    this.to,
  });
  final MuninnApi api;
  final bool unlocked;
  final String? source, symbol, duration;
  final int? from, to;
  @override
  State<_ExportDialog> createState() => _ExportDialogState();
}

class _ExportDialogState extends State<_ExportDialog> {
  late final path = TextEditingController();
  late final source = TextEditingController(text: widget.source ?? '');
  late final symbol = TextEditingController(text: widget.symbol ?? '');
  late final duration = TextEditingController(text: widget.duration ?? '');
  String format = 'csv';
  bool running = false;
  ExportResult? result;
  String? error;

  @override
  void dispose() {
    path.dispose();
    source.dispose();
    symbol.dispose();
    duration.dispose();
    super.dispose();
  }

  Future<void> _browse() async {
    try {
      final picked = await FilePicker.platform.saveFile(fileName: 'muninn-export.$format', type: FileType.custom, allowedExtensions: [format]);
      if (picked != null) setState(() => path.text = picked);
    } catch (e) {
      if (mounted) setState(() => error = 'Could not open the picker. Type a path instead.');
    }
  }

  Future<void> _start() async {
    var p = path.text.trim();
    if (p.isEmpty) {
      setState(() => error = 'Choose where to write the file.');
      return;
    }
    if (!widget.unlocked) {
      setState(() => error = 'Unlock the store first.');
      return;
    }
    if (!p.toLowerCase().endsWith('.$format')) p = '$p.$format';
    setState(() {
      running = true;
      error = null;
      result = null;
    });
    try {
      final r = await widget.api.exportBars(
        path: p,
        format: format,
        source: _opt(source),
        symbol: _opt(symbol),
        duration: _opt(duration),
        from: widget.from?.toString(),
        to: widget.to?.toString(),
      );
      if (mounted) setState(() => result = r);
    } catch (e) {
      if (mounted) setState(() => error = e.toString());
    } finally {
      if (mounted) setState(() => running = false);
    }
  }

  String? _opt(TextEditingController c) {
    final t = c.text.trim();
    return t.isEmpty ? null : t;
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      title: Row(children: [
        Expanded(child: Text('Export', style: TextStyle(color: m.header, fontWeight: FontWeight.w800))),
        IconButton(tooltip: 'Close', icon: const Text('[X]'), onPressed: running ? null : () => Navigator.pop(context)),
      ]),
      content: SizedBox(
        width: 520,
        child: result != null
            ? Text('Wrote ${result!.rows} bar${result!.rows == 1 ? '' : 's'} to ${result!.path} (${result!.format}).', style: TextStyle(color: m.success))
            : Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                Text('Writes stored bars through Polars, as `mnnn export`.', style: TextStyle(color: m.muted)),
                const SizedBox(height: 12),
                SegmentedButton<String>(
                  segments: const [
                    ButtonSegment(value: 'csv', label: Text('CSV')),
                    ButtonSegment(value: 'parquet', label: Text('Parquet')),
                    ButtonSegment(value: 'json', label: Text('JSON')),
                  ],
                  selected: {format},
                  onSelectionChanged: running ? null : (s) => setState(() => format = s.first),
                ),
                const SizedBox(height: 12),
                TextField(
                  key: const ValueKey('export-path'),
                  controller: path,
                  enabled: !running,
                  decoration: InputDecoration(labelText: 'Output file', hintText: 'export.$format'),
                ),
                const SizedBox(height: 8),
                Align(
                  alignment: Alignment.centerLeft,
                  child: OutlinedButton(onPressed: running ? null : _browse, child: const Text('Browse…')),
                ),
                const SizedBox(height: 8),
                TextField(controller: source, enabled: !running, decoration: const InputDecoration(labelText: 'Source (optional)')),
                TextField(controller: symbol, enabled: !running, decoration: const InputDecoration(labelText: 'Symbol (optional)', hintText: 'BTC/USD')),
                TextField(controller: duration, enabled: !running, decoration: const InputDecoration(labelText: 'Duration (optional)', hintText: '5m')),
                if (widget.from != null || widget.to != null) ...[
                  const SizedBox(height: 8),
                  Text(
                    'Time range: '
                    '${widget.from == null ? '…' : '${fmtUtc(widget.from!)} UTC'}'
                    ' → '
                    '${widget.to == null ? '…' : '${fmtUtc(widget.to!)} UTC'}',
                    style: TextStyle(color: m.muted),
                  ),
                ],
                if (!widget.unlocked) ...[
                  const SizedBox(height: 12),
                  Text('Unlock the store first (Home).', style: TextStyle(color: m.warning)),
                ],
                if (running) ...[
                  const SizedBox(height: 16),
                  const LinearProgressIndicator(),
                ],
                if (error != null) ...[
                  const SizedBox(height: 12),
                  Text(error!, style: TextStyle(color: m.error)),
                ],
              ]),
      ),
      actions: [
        if (result != null)
          TextButton(onPressed: () => Navigator.pop(context), child: const Text('Close'))
        else ...[
          TextButton(onPressed: running ? null : () => Navigator.pop(context), child: const Text('Close')),
          FilledButton(key: const ValueKey('export-start'), onPressed: running ? null : _start, child: const Text('Export')),
        ],
      ],
    );
  }
}
