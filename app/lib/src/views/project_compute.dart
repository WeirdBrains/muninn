/// Code spaces, live pipes and pipelines (`mnnn code` / `mnnn pipe` /
/// `mnnn pipeline` and the TUI Code / Pipes / Pipelines windows). Source the
/// user wrote is shown so they can edit it; compiled output stays in Rust.
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

Future<void> _showLines(
  BuildContext context,
  String title,
  List<String> lines, {
  bool error = false,
}) async {
  if (!context.mounted) return;
  final m = context.muninn;
  await showDialog<void>(
    context: context,
    builder: (ctx) => AlertDialog(
      backgroundColor: m.bg,
      title: Text(title, style: TextStyle(color: error ? m.error : m.header)),
      content: SizedBox(
        width: 520,
        height: 360,
        child: SelectableText(
          lines.isEmpty ? '(no output)' : lines.join('\n'),
          style: TextStyle(
            color: error ? m.error : m.fg,
            fontFamily: 'monospace',
          ),
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

Future<void> showNewCodeSpaceDialog(BuildContext context, WidgetRef ref) async {
  ref.read(sectionProvider.notifier).state = Section.projects;
  ref.read(projectPaneProvider.notifier).state = ProjectPane.code;
  if (!ref.read(unlockedProvider)) return;
  final created = await showDialog<bool>(
    context: context,
    builder: (ctx) => const _CodeSpaceDialog(),
  );
  if (created == true) refreshProjectExtras(ref);
}

Future<void> showNewPipelineDialog(BuildContext context, WidgetRef ref) async {
  ref.read(sectionProvider.notifier).state = Section.projects;
  ref.read(projectPaneProvider.notifier).state = ProjectPane.pipelines;
  if (!ref.read(unlockedProvider)) return;
  final created = await showDialog<bool>(
    context: context,
    builder: (ctx) => const _PipelineDialog(),
  );
  if (created == true) refreshProjectExtras(ref);
}

Future<void> showRunPipeDialog(BuildContext context, WidgetRef ref) async {
  ref.read(sectionProvider.notifier).state = Section.projects;
  ref.read(projectPaneProvider.notifier).state = ProjectPane.pipes;
  if (!ref.read(unlockedProvider)) return;
  final ran = await showDialog<bool>(
    context: context,
    builder: (ctx) => const _RunPipeDialog(),
  );
  if (ran == true) refreshProjectExtras(ref);
}

class CodeSpacesPane extends ConsumerWidget {
  const CodeSpacesPane({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return ref
        .watch(codeSpacesProvider)
        .when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Padding(
            padding: const EdgeInsets.all(16),
            child: Text('$e', style: TextStyle(color: m.error)),
          ),
          data: (items) {
            final all = items ?? const <CodeSpaceInfo>[];
            return ListView(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
              children: [
                Text(
                  'Code spaces compile and run over your data (math, polars, rust, python or a prebuilt plugin). Review the source, then approve it before it runs — it runs as native code with your rights. Built-in parsers (parse-csv, to-bars, …) turn stored ingest data into bars.',
                  style: TextStyle(color: m.muted),
                ),
                const SizedBox(height: 12),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    FilledButton.icon(
                      key: const ValueKey('code-new'),
                      onPressed: () => showNewCodeSpaceDialog(context, ref),
                      icon: const Icon(Icons.add),
                      label: const Text('New code space…'),
                    ),
                    OutlinedButton(
                      key: const ValueKey('code-parsers'),
                      onPressed: () => _showParsers(context, ref),
                      child: const Text('Parsers…'),
                    ),
                  ],
                ),
                const SizedBox(height: 12),
                if (all.isEmpty)
                  Text(
                    'No code spaces yet. Create one, or install a built-in parser.',
                    style: TextStyle(color: m.muted),
                  )
                else
                  for (final c in all)
                    ListTile(
                      key: ValueKey('code-${c.id}'),
                      contentPadding: EdgeInsets.zero,
                      title: Text(c.name),
                      subtitle: Text(
                        [
                          c.lang,
                          c.scope,
                          c.approved ? 'approved' : 'not approved',
                          if (c.lastStatus.isNotEmpty) 'last ${c.lastStatus}',
                          if (c.description.isNotEmpty) c.description,
                        ].join(' · '),
                        style: TextStyle(color: m.muted),
                      ),
                      trailing: PopupMenuButton<String>(
                        onSelected: (v) => _codeAction(context, ref, c, v),
                        itemBuilder: (_) => const [
                          PopupMenuItem(value: 'open', child: Text('Open…')),
                          PopupMenuItem(value: 'run', child: Text('Run')),
                          PopupMenuItem(
                            value: 'approve',
                            child: Text('Approve'),
                          ),
                          PopupMenuItem(value: 'check', child: Text('Check')),
                          PopupMenuItem(value: 'build', child: Text('Build')),
                          PopupMenuItem(
                            value: 'give',
                            child: Text('Give to org…'),
                          ),
                          PopupMenuItem(value: 'remove', child: Text('Remove')),
                        ],
                      ),
                    ),
              ],
            );
          },
        );
  }
}

Future<void> _codeAction(
  BuildContext context,
  WidgetRef ref,
  CodeSpaceInfo c,
  String v,
) async {
  final m = context.muninn;
  try {
    switch (v) {
      case 'open':
        final ok = await showDialog<bool>(
          context: context,
          builder: (ctx) => _CodeSpaceDialog(existing: c),
        );
        if (ok == true) refreshProjectExtras(ref);
      case 'run':
        if (!c.approved &&
            !await _confirm(
              context,
              'Approve and run',
              'Code space ${c.name} (${c.lang}) is not approved. It runs as native code with your rights. Approve it and run?',
            )) {
          return;
        }
        final r = await ref.read(apiProvider).codeRun(c.id);
        refreshProjectExtras(ref);
        if (context.mounted) {
          await _showLines(context, '${c.name}: ${r.status}', [
            ...r.logs,
            ...r.lines,
            if (r.error.isNotEmpty) r.error,
          ], error: r.status == 'error' || r.error.isNotEmpty);
        }
      case 'approve':
        final msg = await ref.read(apiProvider).codeApprove(c.id);
        refreshProjectExtras(ref);
        if (context.mounted) _ok(context, msg);
      case 'check':
        final lines = await ref.read(apiProvider).codeCheck(c.id);
        if (context.mounted) {
          await _showLines(context, 'Check ${c.name}', lines);
        }
      case 'build':
        final r = await ref.read(apiProvider).codeBuild(c.id);
        if (context.mounted) {
          await _showLines(context, 'Build ${c.name}', [...r.logs, ...r.lines]);
        }
      case 'give':
        await showCodeGiveToOrgDialog(context, ref, c);
      case 'remove':
        if (!await _confirm(
          context,
          'Remove code space',
          'Remove ${c.name}? Its source stays in the history.',
        )) {
          return;
        }
        final msg = await ref.read(apiProvider).codeRemove(c.id);
        refreshProjectExtras(ref);
        if (context.mounted) _ok(context, msg);
    }
  } catch (e) {
    if (context.mounted) _ok(context, '$e', color: m.error);
  }
}

Future<void> showCodeGiveToOrgDialog(
  BuildContext context,
  WidgetRef ref,
  CodeSpaceInfo c,
) async {
  List<OrgInfo> orgs = const [];
  try {
    orgs = (await ref.read(orgOverviewProvider.future))?.mine ?? const [];
  } catch (e) {
    if (context.mounted) {
      _ok(context, '$e', color: context.muninn.error);
    }
    return;
  }
  if (!context.mounted) {
    return;
  }
  if (orgs.isEmpty) {
    _ok(context, 'Create an organization first.');
    return;
  }
  var slug = orgs.first.slug;
  final ok = await showDialog<bool>(
    context: context,
    builder: (ctx) {
      final m = context.muninn;
      return StatefulBuilder(
        builder: (ctx, setSt) => AlertDialog(
          backgroundColor: m.bg,
          title: Text(
            'Give ${c.name} to an org',
            style: TextStyle(color: m.header),
          ),
          content: SizedBox(
            width: 420,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  'Members pull it with Sync code (`mnnn org sync-code`). This is the TUI Code window g / giving a space to an organization you own.',
                  style: TextStyle(color: m.muted),
                ),
                DropdownButtonFormField<String>(
                  key: const ValueKey('code-give-org'),
                  initialValue: slug,
                  items: [
                    for (final o in orgs)
                      DropdownMenuItem(
                        value: o.slug,
                        child: Text('${o.name} (${o.slug})'),
                      ),
                  ],
                  onChanged: (v) {
                    if (v != null) setSt(() => slug = v);
                  },
                  decoration: const InputDecoration(labelText: 'Organization'),
                ),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(ctx, false),
              child: const Text('Cancel'),
            ),
            FilledButton(
              key: const ValueKey('code-give-save'),
              onPressed: () => Navigator.pop(ctx, true),
              child: const Text('Give'),
            ),
          ],
        ),
      );
    },
  );
  if (ok != true || !context.mounted) {
    return;
  }
  final msg = await ref.read(apiProvider).orgResourceAdd(slug, 'code', c.id);
  refreshProjectExtras(ref);
  if (context.mounted) {
    _ok(context, msg);
  }
}

Future<void> _showParsers(BuildContext context, WidgetRef ref) async {
  try {
    final list = await ref.read(apiProvider).codeParsers();
    if (!context.mounted) return;
    await showDialog<void>(
      context: context,
      builder: (ctx) => _ParsersDialog(parsers: list),
    );
    refreshProjectExtras(ref);
  } catch (e) {
    if (context.mounted) _ok(context, '$e', color: context.muninn.error);
  }
}

class _ParsersDialog extends ConsumerWidget {
  const _ParsersDialog({required this.parsers});
  final List<CodeParserInfo> parsers;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text('Built-in parsers', style: TextStyle(color: m.header)),
      content: SizedBox(
        width: 460,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              'Templates that shape raw ingest data. Install makes them code spaces you can view and edit; approve on first run.',
              style: TextStyle(color: m.muted),
            ),
            const SizedBox(height: 8),
            for (final p in parsers)
              ListTile(
                contentPadding: EdgeInsets.zero,
                title: Text(p.name),
                subtitle: Text(p.about, style: TextStyle(color: m.muted)),
                trailing: Text(p.installed ? 'installed' : 'not installed'),
              ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Close'),
        ),
        FilledButton(
          key: const ValueKey('code-parsers-install'),
          onPressed: () async {
            try {
              final msg = await ref.read(apiProvider).codeParsersInstall();
              if (context.mounted) {
                final host = Navigator.of(context).context;
                Navigator.pop(context);
                refreshProjectExtras(ref);
                if (host.mounted) _ok(host, msg);
              }
            } catch (e) {
              if (context.mounted) _ok(context, '$e', color: m.error);
            }
          },
          child: const Text('Install missing'),
        ),
      ],
    );
  }
}

class _CodeSpaceDialog extends ConsumerStatefulWidget {
  const _CodeSpaceDialog({this.existing});
  final CodeSpaceInfo? existing;
  @override
  ConsumerState<_CodeSpaceDialog> createState() => _CodeSpaceDialogState();
}

class _CodeSpaceDialogState extends ConsumerState<_CodeSpaceDialog> {
  late final TextEditingController _name;
  late final TextEditingController _desc;
  late final TextEditingController _source;
  late final TextEditingController _input;
  late final TextEditingController _from;
  late String _lang;

  @override
  void initState() {
    super.initState();
    final e = widget.existing;
    _name = TextEditingController(text: e?.name ?? '');
    _desc = TextEditingController(text: e?.description ?? '');
    _source = TextEditingController(text: e?.source ?? '');
    _input = TextEditingController(
      text: e?.input == 'none' ? '' : (e?.input ?? ''),
    );
    _from = TextEditingController();
    _lang = e?.lang ?? 'math';
  }

  @override
  void dispose() {
    _name.dispose();
    _desc.dispose();
    _source.dispose();
    _input.dispose();
    _from.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    try {
      if (widget.existing == null) {
        await ref
            .read(apiProvider)
            .codeCreate(
              _name.text.trim(),
              lang: _lang,
              description: _desc.text.trim(),
              source: _source.text.trim().isEmpty ? null : _source.text,
              from: _from.text.trim().isEmpty ? null : _from.text.trim(),
              input: _input.text.trim().isEmpty ? null : _input.text.trim(),
            );
      } else {
        await ref
            .read(apiProvider)
            .codeEdit(
              widget.existing!.id,
              name: _name.text.trim(),
              description: _desc.text.trim(),
              lang: _lang,
              source: _source.text,
              input: _input.text.trim().isEmpty ? 'none' : _input.text.trim(),
            );
      }
      if (mounted) Navigator.pop(context, true);
    } catch (e) {
      if (mounted) _ok(context, '$e', color: context.muninn.error);
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    final editing = widget.existing != null;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text(
        editing ? 'Edit code space' : 'New code space',
        style: TextStyle(color: m.header),
      ),
      content: SizedBox(
        width: 560,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              TextField(
                key: const ValueKey('code-name'),
                controller: _name,
                decoration: const InputDecoration(labelText: 'Name'),
              ),
              DropdownButtonFormField<String>(
                key: const ValueKey('code-lang'),
                initialValue: _lang,
                items: [
                  for (final l in [
                    'math',
                    'polars',
                    'rust',
                    'python',
                    'plugin',
                  ])
                    DropdownMenuItem(value: l, child: Text(l)),
                ],
                onChanged: (v) => setState(() => _lang = v ?? 'math'),
                decoration: const InputDecoration(labelText: 'Language'),
              ),
              TextField(
                controller: _desc,
                decoration: const InputDecoration(labelText: 'Description'),
              ),
              if (!editing)
                TextField(
                  controller: _from,
                  decoration: const InputDecoration(
                    labelText: 'Start from (optional)',
                    hintText: 'another space or a built-in parser (parse-csv, to-bars)',
                  ),
                ),
              TextField(
                controller: _input,
                decoration: const InputDecoration(
                  labelText: 'Input',
                  hintText: 'none, or bars where … limit N, or search:NAME',
                ),
              ),
              TextField(
                key: const ValueKey('code-source'),
                controller: _source,
                maxLines: 10,
                decoration: const InputDecoration(
                  labelText: 'Source',
                  hintText: 'leave empty for the language template',
                ),
              ),
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
          key: const ValueKey('code-save'),
          onPressed: _save,
          child: Text(editing ? 'Save' : 'Create'),
        ),
      ],
    );
  }
}

class PipesPane extends ConsumerWidget {
  const PipesPane({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return ref
        .watch(pipesLiveProvider)
        .when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Padding(
            padding: const EdgeInsets.all(16),
            child: Text('$e', style: TextStyle(color: m.error)),
          ),
          data: (items) {
            final all = items ?? const <LivePipeInfo>[];
            return ListView(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
              children: [
                Text(
                  'Live pipes, from this app or the CLI (`mnnn pipe SOURCE STEP…`). Each hop shows its status, rows and errors. Standard input (`-`) stays in the CLI.',
                  style: TextStyle(color: m.muted),
                ),
                const SizedBox(height: 12),
                Align(
                  alignment: Alignment.centerLeft,
                  child: FilledButton.icon(
                    key: const ValueKey('pipe-run'),
                    onPressed: () => showRunPipeDialog(context, ref),
                    icon: const Icon(Icons.play_arrow),
                    label: const Text('Run a pipe…'),
                  ),
                ),
                const SizedBox(height: 12),
                if (all.isEmpty)
                  Text(
                    'No pipes yet: run `mnnn pipe SOURCE STEP…`, share data into a code space, or use Run a pipe…',
                    style: TextStyle(color: m.muted),
                  )
                else
                  for (final p in all) ...[
                    ListTile(
                      contentPadding: EdgeInsets.zero,
                      title: Text(
                        p.hops
                            .map((h) => h.name)
                            .where((n) => n.isNotEmpty)
                            .join(' → '),
                      ),
                      subtitle: Text(
                        [
                          p.status,
                          if (p.pipeline.isNotEmpty) p.pipeline,
                          p.origin,
                        ].join(' · '),
                        style: TextStyle(
                          color: p.status == 'error'
                              ? m.error
                              : p.status == 'done'
                              ? m.accent
                              : m.warning,
                        ),
                      ),
                    ),
                    for (final h in p.hops)
                      Padding(
                        padding: const EdgeInsets.only(left: 16, bottom: 4),
                        child: Text(
                          [
                            h.name,
                            h.status,
                            if (h.rows > 0) '${h.rows} rows',
                            if (h.ms > 0) '${h.ms} ms',
                            if (h.error.isNotEmpty) h.error,
                            if (h.columns.isNotEmpty) h.columns.join(', '),
                          ].join(' · '),
                          style: TextStyle(color: m.muted),
                        ),
                      ),
                    for (final s in p.saved)
                      Padding(
                        padding: const EdgeInsets.only(left: 16, bottom: 4),
                        child: Text(
                          'saved $s',
                          style: TextStyle(color: m.muted),
                        ),
                      ),
                  ],
              ],
            );
          },
        );
  }
}

class _RunPipeDialog extends ConsumerStatefulWidget {
  const _RunPipeDialog();
  @override
  ConsumerState<_RunPipeDialog> createState() => _RunPipeDialogState();
}

class _RunPipeDialogState extends ConsumerState<_RunPipeDialog> {
  final _source = TextEditingController();
  final _steps = TextEditingController();
  final _to = TextEditingController();

  @override
  void dispose() {
    _source.dispose();
    _steps.dispose();
    _to.dispose();
    super.dispose();
  }

  List<String> _lines(String s) => [
    for (final p in s.split(RegExp(r'[\n,]')))
      if (p.trim().isNotEmpty) p.trim(),
  ];

  Future<void> _run() async {
    try {
      final r = await ref
          .read(apiProvider)
          .pipeRun(
            _source.text.trim(),
            steps: _lines(_steps.text),
            to: _lines(_to.text),
          );
      if (!mounted) return;
      final host = Navigator.of(context).context;
      Navigator.pop(context, true);
      if (host.mounted) {
        await _showLines(host, 'Pipe ${r.hops.join(' | ')}', [
          ...r.lines,
          for (final s in r.saved) 'saved $s',
        ]);
      }
    } catch (e) {
      if (mounted) _ok(context, '$e', color: context.muninn.error);
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text('Run a pipe', style: TextStyle(color: m.header)),
      content: SizedBox(
        width: 480,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              'Source: source:NAME, search:NAME, journal:NAME, code:SPACE or a model query. Steps: code:SPACE (options like code:to-bars?symbol=BTC/USDT&duration=5m), search:NAME, or a save. Final saves go in To.',
              style: TextStyle(color: m.muted),
            ),
            TextField(
              key: const ValueKey('pipe-source'),
              controller: _source,
              decoration: const InputDecoration(
                labelText: 'Source',
                hintText: 'source:coinbase',
              ),
            ),
            TextField(
              key: const ValueKey('pipe-steps'),
              controller: _steps,
              maxLines: 4,
              decoration: const InputDecoration(
                labelText: 'Steps',
                hintText:
                    'code:parse-csv\ncode:to-bars?symbol=BTC/USDT&duration=5m',
              ),
            ),
            TextField(
              controller: _to,
              decoration: const InputDecoration(
                labelText: 'To (optional)',
                hintText: 'model:bars  or journal:Notes',
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
          key: const ValueKey('pipe-run-confirm'),
          onPressed: _run,
          child: const Text('Run'),
        ),
      ],
    );
  }
}

class PipelinesPane extends ConsumerWidget {
  const PipelinesPane({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return ref
        .watch(pipelinesProvider)
        .when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Padding(
            padding: const EdgeInsets.all(16),
            child: Text('$e', style: TextStyle(color: m.error)),
          ),
          data: (items) {
            final all = items ?? const <PipelineInfo>[];
            return ListView(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
              children: [
                Text(
                  'Pipelines are saved graphs: ingest data through code spaces and into models. Nodes are ID=KIND (src=source:coinbase, bars=code:to-bars, out=model:bars); edges are FROM>TO. After ingest stores raw files, bars come from a pipeline.',
                  style: TextStyle(color: m.muted),
                ),
                const SizedBox(height: 12),
                Wrap(
                  spacing: 8,
                  children: [
                    FilledButton.icon(
                      key: const ValueKey('pipeline-new'),
                      onPressed: () => showNewPipelineDialog(context, ref),
                      icon: const Icon(Icons.add),
                      label: const Text('New pipeline…'),
                    ),
                    OutlinedButton(
                      key: const ValueKey('pipeline-import'),
                      onPressed: () => _importPipeline(context, ref),
                      child: const Text('Import…'),
                    ),
                  ],
                ),
                const SizedBox(height: 12),
                if (all.isEmpty)
                  Text(
                    'No pipelines yet (`mnnn pipeline create NAME --node … --edge …`).',
                    style: TextStyle(color: m.muted),
                  )
                else
                  for (final p in all)
                    ExpansionTile(
                      key: ValueKey('pipeline-${p.id}'),
                      tilePadding: EdgeInsets.zero,
                      title: Text(p.name),
                      subtitle: Text(
                        [
                          'v${p.version}',
                          '${p.nodes} node(s)',
                          if (p.sinks.isNotEmpty) '→ ${p.sinks}',
                          if (p.draft) 'draft',
                          if (p.problems.isNotEmpty) 'not valid',
                          if (p.description.isNotEmpty) p.description,
                        ].join(' · '),
                        style: TextStyle(
                          color: p.draft || p.problems.isNotEmpty
                              ? m.warning
                              : m.muted,
                        ),
                      ),
                      trailing: PopupMenuButton<String>(
                        onSelected: (v) => _pipelineAction(context, ref, p, v),
                        itemBuilder: (_) => [
                          const PopupMenuItem(
                            value: 'edit',
                            child: Text('Edit…'),
                          ),
                          const PopupMenuItem(value: 'run', child: Text('Run')),
                          if (p.draft)
                            const PopupMenuItem(
                              value: 'approve',
                              child: Text('Approve'),
                            ),
                          const PopupMenuItem(
                            value: 'export',
                            child: Text('Export…'),
                          ),
                          const PopupMenuItem(
                            value: 'remove',
                            child: Text('Remove'),
                          ),
                        ],
                      ),
                      children: [
                        for (final l in p.graphLines)
                          Align(
                            alignment: Alignment.centerLeft,
                            child: Padding(
                              padding: const EdgeInsets.only(bottom: 4),
                              child: Text(
                                l,
                                style: TextStyle(
                                  color: l.startsWith('!') ? m.error : m.fg,
                                  fontFamily: 'monospace',
                                ),
                              ),
                            ),
                          ),
                      ],
                    ),
              ],
            );
          },
        );
  }
}

Future<void> _pipelineAction(
  BuildContext context,
  WidgetRef ref,
  PipelineInfo p,
  String v,
) async {
  final m = context.muninn;
  try {
    switch (v) {
      case 'edit':
        final ok = await showDialog<bool>(
          context: context,
          builder: (ctx) => _PipelineDialog(existing: p),
        );
        if (ok == true) refreshProjectExtras(ref);
      case 'run':
        final r = await ref.read(apiProvider).pipelineRun(p.id);
        refreshProjectExtras(ref);
        if (context.mounted) {
          await _showLines(context, '${p.name}: ${r.ok ? "ok" : "errors"}', [
            ...r.lines,
            ...r.landed,
          ], error: !r.ok);
        }
      case 'approve':
        final msg = await ref.read(apiProvider).pipelineApprove(p.id);
        refreshProjectExtras(ref);
        if (context.mounted) _ok(context, msg);
      case 'export':
        final path = await FilePicker.platform.saveFile(
          fileName: '${p.name}.json',
        );
        if (path == null || !context.mounted) return;
        final msg = await ref.read(apiProvider).pipelineExport(p.id, path);
        if (context.mounted) _ok(context, msg);
      case 'remove':
        if (!await _confirm(context, 'Remove pipeline', 'Remove ${p.name}?')) {
          return;
        }
        final msg = await ref.read(apiProvider).pipelineRemove(p.id);
        refreshProjectExtras(ref);
        if (context.mounted) _ok(context, msg);
    }
  } catch (e) {
    if (context.mounted) _ok(context, '$e', color: m.error);
  }
}

Future<void> _importPipeline(BuildContext context, WidgetRef ref) async {
  final picked = await FilePicker.platform.pickFiles();
  final path = picked?.files.single.path;
  if (path == null || !context.mounted) return;
  try {
    final plan = await ref.read(apiProvider).pipelineImportPlan(path);
    if (!context.mounted) return;
    if (!await _confirm(context, 'Import pipeline', plan.join('\n'))) return;
    final msg = await ref.read(apiProvider).pipelineImport(path);
    refreshProjectExtras(ref);
    if (context.mounted) _ok(context, msg);
  } catch (e) {
    if (context.mounted) _ok(context, '$e', color: context.muninn.error);
  }
}

class _PipelineDialog extends ConsumerStatefulWidget {
  const _PipelineDialog({this.existing});
  final PipelineInfo? existing;
  @override
  ConsumerState<_PipelineDialog> createState() => _PipelineDialogState();
}

class _PipelineDialogState extends ConsumerState<_PipelineDialog> {
  late final TextEditingController _name;
  late final TextEditingController _desc;
  late final TextEditingController _nodes;
  late final TextEditingController _edges;
  late final TextEditingController _removeNodes;
  late final TextEditingController _removeEdges;

  @override
  void initState() {
    super.initState();
    final e = widget.existing;
    _name = TextEditingController(text: e?.name ?? '');
    _desc = TextEditingController(text: e?.description ?? '');
    _nodes = TextEditingController(text: e?.nodeSpecs.join('\n') ?? '');
    _edges = TextEditingController(text: e?.edgeSpecs.join('\n') ?? '');
    _removeNodes = TextEditingController();
    _removeEdges = TextEditingController();
  }

  @override
  void dispose() {
    _name.dispose();
    _desc.dispose();
    _nodes.dispose();
    _edges.dispose();
    _removeNodes.dispose();
    _removeEdges.dispose();
    super.dispose();
  }

  List<String> _lines(String s) => [
    for (final p in s.split('\n'))
      if (p.trim().isNotEmpty) p.trim(),
  ];

  Future<void> _save() async {
    try {
      if (widget.existing == null) {
        await ref
            .read(apiProvider)
            .pipelineCreate(
              _name.text.trim(),
              description: _desc.text.trim(),
              nodes: _lines(_nodes.text),
              edges: _lines(_edges.text),
            );
      } else {
        await ref
            .read(apiProvider)
            .pipelineEdit(
              widget.existing!.id,
              name: _name.text.trim(),
              description: _desc.text.trim(),
              nodes: _lines(_nodes.text),
              edges: _lines(_edges.text),
              removeNodes: _lines(_removeNodes.text),
              removeEdges: _lines(_removeEdges.text),
            );
      }
      if (mounted) Navigator.pop(context, true);
    } catch (e) {
      if (mounted) _ok(context, '$e', color: context.muninn.error);
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    final editing = widget.existing != null;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text(
        editing ? 'Edit pipeline' : 'New pipeline',
        style: TextStyle(color: m.header),
      ),
      content: SizedBox(
        width: 560,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              TextField(
                key: const ValueKey('pipeline-name'),
                controller: _name,
                decoration: const InputDecoration(labelText: 'Name'),
              ),
              TextField(
                controller: _desc,
                decoration: const InputDecoration(labelText: 'Description'),
              ),
              TextField(
                key: const ValueKey('pipeline-nodes'),
                controller: _nodes,
                maxLines: 5,
                decoration: const InputDecoration(
                  labelText: 'Nodes',
                  hintText:
                      'src=source:coinbase\nbars=code:to-bars\nout=model:bars',
                ),
              ),
              TextField(
                key: const ValueKey('pipeline-edges'),
                controller: _edges,
                maxLines: 4,
                decoration: const InputDecoration(
                  labelText: 'Edges',
                  hintText: 'src>bars\nbars>out',
                ),
              ),
              if (editing) ...[
                TextField(
                  controller: _removeNodes,
                  decoration: const InputDecoration(
                    labelText: 'Remove nodes (optional)',
                  ),
                ),
                TextField(
                  controller: _removeEdges,
                  decoration: const InputDecoration(
                    labelText: 'Remove edges (optional)',
                    hintText: 'FROM>TO',
                  ),
                ),
              ],
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
          key: const ValueKey('pipeline-save'),
          onPressed: _save,
          child: Text(editing ? 'Save' : 'Create'),
        ),
      ],
    );
  }
}
