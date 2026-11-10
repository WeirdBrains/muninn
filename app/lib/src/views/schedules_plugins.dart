/// Plugins by kind, rule sets and schedules (`mnnn plugins` / `mnnn ruleset` /
/// `mnnn schedule` and the TUI Plugins / Rules ▸ Rule sets / Schedules).
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../state.dart';
import '../theme/muninn_theme.dart';
import 'shared_actions.dart';

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

Future<void> showPluginsPane(
  BuildContext context,
  WidgetRef ref, {
  String? kind,
}) async {
  ref.read(pluginKindFilterProvider.notifier).state = kind;
  ref.read(sectionProvider.notifier).state = Section.projects;
  ref.read(projectPaneProvider.notifier).state = ProjectPane.plugins;
}

Future<void> showNewScheduleDialog(BuildContext context, WidgetRef ref) async {
  ref.read(sectionProvider.notifier).state = Section.projects;
  ref.read(projectPaneProvider.notifier).state = ProjectPane.schedules;
  if (!ref.read(unlockedProvider)) return;
  final ok = await showDialog<bool>(
    context: context,
    builder: (ctx) => const _ScheduleFormDialog(),
  );
  if (ok == true) refreshProjectExtras(ref);
}

Future<void> showNewRulesetDialog(BuildContext context, WidgetRef ref) async {
  ref.read(sectionProvider.notifier).state = Section.projects;
  ref.read(projectPaneProvider.notifier).state = ProjectPane.rulesets;
  if (!ref.read(unlockedProvider)) return;
  final ok = await showDialog<bool>(
    context: context,
    builder: (ctx) => const _RulesetFormDialog(),
  );
  if (ok == true) refreshProjectExtras(ref);
}

class PluginsPane extends ConsumerWidget {
  const PluginsPane({super.key});

  static const kinds = [
    (null, 'All plugins'),
    ('parser', 'Parsers'),
    ('transform', 'Transforms'),
    ('analysis', 'Analysis'),
    ('ingest-connector', 'Ingest connectors'),
  ];

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final filter = ref.watch(pluginKindFilterProvider);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.only(bottom: 8),
          child: Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              for (final (k, label) in kinds)
                FilterChip(
                  label: Text(label),
                  selected: filter == k,
                  onSelected: (_) {
                    ref.read(pluginKindFilterProvider.notifier).state = k;
                    ref.invalidate(pluginsProvider);
                  },
                ),
            ],
          ),
        ),
        Align(
          alignment: Alignment.centerLeft,
          child: OutlinedButton(
            key: const ValueKey('plugin-install-open'),
            onPressed: () => showPluginInstallDialog(context, ref),
            child: const Text('Install plugin…'),
          ),
        ),
        const SizedBox(height: 8),
        Text(
          'Plugins are code spaces by kind. Install one from a library or an existing code name, then review and approve it. A schedule that uses a plugin refuses removal.',
          style: TextStyle(color: m.muted),
        ),
        const SizedBox(height: 8),
        Expanded(
          child: ref
              .watch(pluginsProvider)
              .when(
                loading: () => const LinearProgressIndicator(),
                error: (e, _) => Text('$e', style: TextStyle(color: m.error)),
                data: (items) {
                  final all = items ?? const <PluginInfo>[];
                  if (all.isEmpty) {
                    return Text(
                      'No plugins${filter == null ? '' : ' of kind $filter'}.',
                      style: TextStyle(color: m.muted),
                    );
                  }
                  return ListView(
                    children: [
                      for (final p in all)
                        ListTile(
                          key: ValueKey('plugin-${p.name}'),
                          contentPadding: EdgeInsets.zero,
                          title: Text(p.name),
                          subtitle: Text(
                            [
                              p.kind,
                              p.lang,
                              if (p.main.isNotEmpty) 'main ${p.main}',
                              p.approved ? 'approved' : 'not approved',
                              p.enabled ? 'on' : 'off',
                            ].join(' · '),
                            style: TextStyle(color: m.muted),
                          ),
                          trailing: PopupMenuButton<String>(
                            onSelected: (v) async {
                              try {
                                if (v == 'enable' || v == 'disable') {
                                  final msg = await ref
                                      .read(apiProvider)
                                      .pluginSetEnabled(p.name, v == 'enable');
                                  refreshProjectExtras(ref);
                                  if (context.mounted) _ok(context, msg);
                                } else if (v == 'schedule' &&
                                    p.kind == 'ingest-connector') {
                                  ref.read(projectPaneProvider.notifier).state =
                                      ProjectPane.schedules;
                                  await showDialog<bool>(
                                    context: context,
                                    builder: (ctx) => _ScheduleFormDialog(
                                      presetPlugin: p.name,
                                    ),
                                  );
                                  refreshProjectExtras(ref);
                                } else if (v == 'info') {
                                  final d = await ref
                                      .read(apiProvider)
                                      .pluginInfo(p.name);
                                  if (context.mounted) {
                                    await _showLines(context, d.name, [
                                      'kind ${d.kind}',
                                      'main ${d.main} (${d.lang})',
                                      if (d.library.isNotEmpty)
                                        'library ${d.library}',
                                      if (d.declared.isNotEmpty)
                                        'declares ${d.declared}',
                                      '${d.members} member(s)',
                                      d.approved ? 'approved' : 'not approved',
                                      d.enabled ? 'enabled' : 'disabled',
                                      'used by: ${d.usedBy.isEmpty ? 'nothing yet' : d.usedBy.join(', ')}',
                                      if (d.problem.isNotEmpty)
                                        'problem: ${d.problem}',
                                    ]);
                                  }
                                } else if (v == 'open') {
                                  ref.read(projectPaneProvider.notifier).state =
                                      ProjectPane.spaces;
                                } else if (v == 'remove') {
                                  if (!context.mounted) return;
                                  if (!await _confirm(
                                    context,
                                    'Remove plugin',
                                    'Remove ${p.name}? A schedule that uses it will refuse.',
                                  )) {
                                    return;
                                  }
                                  final msg = await ref
                                      .read(apiProvider)
                                      .collectionRemove(p.name);
                                  refreshProjectExtras(ref);
                                  if (context.mounted) _ok(context, msg);
                                }
                              } catch (e) {
                                if (context.mounted) {
                                  _ok(context, '$e', color: m.error);
                                }
                              }
                            },
                            itemBuilder: (_) => [
                              const PopupMenuItem(
                                value: 'info',
                                child: Text('Info…'),
                              ),
                              const PopupMenuItem(
                                value: 'open',
                                child: Text('Open members…'),
                              ),
                              if (!p.enabled)
                                const PopupMenuItem(
                                  value: 'enable',
                                  child: Text('Enable'),
                                ),
                              if (p.enabled)
                                const PopupMenuItem(
                                  value: 'disable',
                                  child: Text('Disable'),
                                ),
                              if (p.kind == 'ingest-connector')
                                const PopupMenuItem(
                                  value: 'schedule',
                                  child: Text('Schedule…'),
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
              ),
        ),
      ],
    );
  }
}

class RulesetsPane extends ConsumerWidget {
  const RulesetsPane({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Text('Rule sets', style: TextStyle(color: m.header, fontSize: 18)),
            const Spacer(),
            FilledButton.tonal(
              onPressed: () => showNewRulesetDialog(context, ref),
              child: const Text('New rule set…'),
            ),
          ],
        ),
        const SizedBox(height: 8),
        Expanded(
          child: ref
              .watch(rulesetsProvider)
              .when(
                loading: () => const LinearProgressIndicator(),
                error: (e, _) => Text('$e', style: TextStyle(color: m.error)),
                data: (items) {
                  final all = items ?? const <RulesetInfo>[];
                  if (all.isEmpty) {
                    return Text(
                      'No rule sets yet. Create one, then attach schedules.',
                      style: TextStyle(color: m.muted),
                    );
                  }
                  return ListView(
                    children: [
                      for (final r in all)
                        ListTile(
                          key: ValueKey('ruleset-${r.id}'),
                          contentPadding: EdgeInsets.zero,
                          title: Text('${r.name}${r.enabled ? '' : ' (off)'}'),
                          subtitle: Text(
                            [
                              r.ownerKind,
                              '${r.rules.length} rule(s)',
                              '${r.schedules.length} schedule(s)',
                              if (r.description.isNotEmpty) r.description,
                            ].join(' · '),
                            style: TextStyle(color: m.muted),
                          ),
                          trailing: PopupMenuButton<String>(
                            onSelected: (v) async {
                              try {
                                if (v == 'enable' || v == 'disable') {
                                  await ref
                                      .read(apiProvider)
                                      .rulesetEdit(r.id, enable: v == 'enable');
                                  refreshProjectExtras(ref);
                                } else if (v == 'remove') {
                                  if (!await _confirm(
                                    context,
                                    'Remove rule set',
                                    'Remove ${r.name}? Its rules stay.',
                                  )) {
                                    return;
                                  }
                                  final msg = await ref
                                      .read(apiProvider)
                                      .rulesetRemove(r.id, force: true);
                                  refreshProjectExtras(ref);
                                  if (context.mounted) _ok(context, msg);
                                } else if (v == 'rules') {
                                  await showDialog<void>(
                                    context: context,
                                    builder: (ctx) =>
                                        _RulesetRulesDialog(set: r),
                                  );
                                  refreshProjectExtras(ref);
                                } else if (v == 'run') {
                                  final lines = await ref
                                      .read(apiProvider)
                                      .rulesetRun(r.id);
                                  if (context.mounted) {
                                    await _showLines(
                                      context,
                                      'Run ${r.name}',
                                      lines,
                                    );
                                  }
                                } else if (v == 'schedule') {
                                  await showDialog<bool>(
                                    context: context,
                                    builder: (ctx) => _ScheduleFormDialog(
                                      presetRuleset: r.name,
                                    ),
                                  );
                                  refreshProjectExtras(ref);
                                }
                              } catch (e) {
                                if (context.mounted) {
                                  _ok(context, '$e', color: m.error);
                                }
                              }
                            },
                            itemBuilder: (_) => [
                              PopupMenuItem(
                                value: r.enabled ? 'disable' : 'enable',
                                child: Text(r.enabled ? 'Disable' : 'Enable'),
                              ),
                              const PopupMenuItem(
                                value: 'rules',
                                child: Text('Rules…'),
                              ),
                              const PopupMenuItem(
                                value: 'run',
                                child: Text('Run'),
                              ),
                              const PopupMenuItem(
                                value: 'schedule',
                                child: Text('New schedule…'),
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
              ),
        ),
      ],
    );
  }
}

class SchedulesPane extends ConsumerWidget {
  const SchedulesPane({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Text('Schedules', style: TextStyle(color: m.header, fontSize: 18)),
            const Spacer(),
            FilledButton.tonal(
              onPressed: () => showNewScheduleDialog(context, ref),
              child: const Text('New schedule…'),
            ),
            const SizedBox(width: 8),
            OutlinedButton(
              onPressed: () async {
                try {
                  final msg = await ref.read(apiProvider).schedulerTick();
                  if (context.mounted) _ok(context, msg);
                } catch (e) {
                  if (context.mounted) _ok(context, '$e', color: m.error);
                }
              },
              child: const Text('Tick now'),
            ),
          ],
        ),
        const SizedBox(height: 8),
        Expanded(
          child: ref
              .watch(schedulesProvider)
              .when(
                loading: () => const LinearProgressIndicator(),
                error: (e, _) => Text('$e', style: TextStyle(color: m.error)),
                data: (items) {
                  final all = items ?? const <ScheduleInfo>[];
                  if (all.isEmpty) {
                    return Text(
                      'No schedules. New schedule… targets a rule set or an ingest connector.',
                      style: TextStyle(color: m.muted),
                    );
                  }
                  return ListView(
                    children: [
                      for (final s in all)
                        ListTile(
                          key: ValueKey('schedule-${s.id}'),
                          contentPadding: EdgeInsets.zero,
                          title: Text('${s.name}${s.enabled ? '' : ' (off)'}'),
                          subtitle: Text(
                            [
                              '${s.targetKind} ${s.target}',
                              ...s.describe,
                            ].join(' · '),
                            style: TextStyle(color: m.muted),
                          ),
                          trailing: PopupMenuButton<String>(
                            onSelected: (v) =>
                                _scheduleAction(context, ref, s, v),
                            itemBuilder: (_) => [
                              const PopupMenuItem(
                                value: 'edit',
                                child: Text('Edit…'),
                              ),
                              const PopupMenuItem(
                                value: 'run',
                                child: Text('Run now'),
                              ),
                              const PopupMenuItem(
                                value: 'runs',
                                child: Text('Runs…'),
                              ),
                              PopupMenuItem(
                                value: s.enabled ? 'disable' : 'enable',
                                child: Text(s.enabled ? 'Disable' : 'Enable'),
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
              ),
        ),
      ],
    );
  }
}

Future<void> _scheduleAction(
  BuildContext context,
  WidgetRef ref,
  ScheduleInfo s,
  String v,
) async {
  final m = context.muninn;
  try {
    switch (v) {
      case 'edit':
        final ok = await showDialog<bool>(
          context: context,
          builder: (ctx) => _ScheduleFormDialog(existing: s),
        );
        if (ok == true) refreshProjectExtras(ref);
      case 'run':
        final r = await ref.read(apiProvider).scheduleRunNow(s.id);
        refreshProjectExtras(ref);
        if (context.mounted) {
          _ok(context, 'run #${r.id} ${r.status}');
        }
      case 'runs':
        final runs = await ref.read(apiProvider).scheduleRuns(s.id);
        if (!context.mounted) return;
        await showDialog<void>(
          context: context,
          builder: (ctx) => AlertDialog(
            backgroundColor: m.bg,
            title: Text('Runs · ${s.name}', style: TextStyle(color: m.header)),
            content: SizedBox(
              width: 480,
              height: 320,
              child: ListView(
                children: [
                  for (final r in runs)
                    Text(
                      '#${r.id} ${r.status} attempt ${r.attempt}'
                      '${r.error.isEmpty ? '' : ': ${r.error}'}',
                    ),
                  if (runs.isEmpty)
                    Text('No runs yet.', style: TextStyle(color: m.muted)),
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
      case 'enable':
      case 'disable':
        await ref.read(apiProvider).scheduleSetEnabled(s.id, v == 'enable');
        refreshProjectExtras(ref);
      case 'remove':
        if (!await _confirm(context, 'Remove schedule', 'Remove ${s.name}?')) {
          return;
        }
        final msg = await ref.read(apiProvider).scheduleRemove(s.id);
        refreshProjectExtras(ref);
        if (context.mounted) _ok(context, msg);
    }
  } catch (e) {
    if (context.mounted) _ok(context, '$e', color: m.error);
  }
}

Future<void> _showLines(
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

class _RulesetRulesDialog extends ConsumerStatefulWidget {
  const _RulesetRulesDialog({required this.set});
  final RulesetInfo set;
  @override
  ConsumerState<_RulesetRulesDialog> createState() =>
      _RulesetRulesDialogState();
}

class _RulesetRulesDialogState extends ConsumerState<_RulesetRulesDialog> {
  late List<String> _rules = List.of(widget.set.rules);
  final _add = TextEditingController();
  var _busy = false;

  @override
  void dispose() {
    _add.dispose();
    super.dispose();
  }

  Future<void> _reload() async {
    final all = await ref.read(apiProvider).rulesetsList();
    RulesetInfo? cur;
    for (final r in all) {
      if (r.id == widget.set.id) cur = r;
    }
    if (cur != null && mounted) setState(() => _rules = List.of(cur!.rules));
  }

  Future<void> _go(Future<void> Function() call) async {
    final m = context.muninn;
    setState(() => _busy = true);
    try {
      await call();
      await _reload();
    } catch (e) {
      if (mounted) _ok(context, '$e', color: m.error);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text(
        '${widget.set.name} · rules',
        style: TextStyle(color: m.header),
      ),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            for (var i = 0; i < _rules.length; i++)
              ListTile(
                contentPadding: EdgeInsets.zero,
                title: Text(_rules[i]),
                trailing: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    IconButton(
                      tooltip: 'Earlier',
                      onPressed: _busy || i == 0
                          ? null
                          : () => _go(() async {
                              await ref
                                  .read(apiProvider)
                                  .rulesetAddRule(
                                    widget.set.id,
                                    _rules[i],
                                    position: i,
                                  );
                            }),
                      icon: const Icon(Icons.arrow_upward),
                    ),
                    IconButton(
                      tooltip: 'Later',
                      onPressed: _busy || i == _rules.length - 1
                          ? null
                          : () => _go(() async {
                              await ref
                                  .read(apiProvider)
                                  .rulesetAddRule(
                                    widget.set.id,
                                    _rules[i],
                                    position: i + 2,
                                  );
                            }),
                      icon: const Icon(Icons.arrow_downward),
                    ),
                    IconButton(
                      tooltip: 'Drop',
                      onPressed: _busy
                          ? null
                          : () => _go(() async {
                              await ref
                                  .read(apiProvider)
                                  .rulesetDropRule(widget.set.id, _rules[i]);
                            }),
                      icon: const Icon(Icons.close),
                    ),
                  ],
                ),
              ),
            TextField(
              key: const ValueKey('ruleset-add-rule'),
              controller: _add,
              decoration: const InputDecoration(labelText: 'Add rule (name)'),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Close'),
        ),
        TextButton(
          onPressed: _busy
              ? null
              : () => _go(() async {
                  final name = _add.text.trim();
                  if (name.isEmpty) return;
                  await ref
                      .read(apiProvider)
                      .rulesetAddRule(widget.set.id, name);
                  _add.clear();
                }),
          child: const Text('Add'),
        ),
      ],
    );
  }
}

class _RulesetFormDialog extends ConsumerStatefulWidget {
  const _RulesetFormDialog();
  @override
  ConsumerState<_RulesetFormDialog> createState() => _RulesetFormDialogState();
}

class _RulesetFormDialogState extends ConsumerState<_RulesetFormDialog> {
  final _name = TextEditingController();
  final _description = TextEditingController();
  final _rules = TextEditingController();
  var _busy = false;

  @override
  void dispose() {
    _name.dispose();
    _description.dispose();
    _rules.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text('New rule set', style: TextStyle(color: m.header)),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              key: const ValueKey('ruleset-name'),
              controller: _name,
              decoration: const InputDecoration(labelText: 'Name'),
            ),
            TextField(
              controller: _description,
              decoration: const InputDecoration(labelText: 'Description'),
            ),
            TextField(
              controller: _rules,
              decoration: const InputDecoration(
                labelText: 'Rules (comma-separated names)',
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
          key: const ValueKey('ruleset-save'),
          onPressed: _busy
              ? null
              : () async {
                  setState(() => _busy = true);
                  try {
                    final rules = _rules.text
                        .split(',')
                        .map((s) => s.trim())
                        .where((s) => s.isNotEmpty)
                        .toList();
                    await ref
                        .read(apiProvider)
                        .rulesetCreate(
                          _name.text.trim(),
                          description: _description.text.trim(),
                          rules: rules,
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

class _ScheduleFormDialog extends ConsumerStatefulWidget {
  const _ScheduleFormDialog({
    this.existing,
    this.presetRuleset,
    this.presetPlugin,
  });
  final ScheduleInfo? existing;
  final String? presetRuleset;
  final String? presetPlugin;

  @override
  ConsumerState<_ScheduleFormDialog> createState() =>
      _ScheduleFormDialogState();
}

class _ScheduleFormDialogState extends ConsumerState<_ScheduleFormDialog> {
  late final TextEditingController _name;
  late final TextEditingController _target;
  late final TextEditingController _source;
  late final TextEditingController _startAt;
  late final TextEditingController _repeat;
  late final TextEditingController _cron;
  late final TextEditingController _endsAt;
  late final TextEditingController _for;
  late final TextEditingController _tz;
  late final TextEditingController _pipeline;
  late final TextEditingController _ruleset;
  late final TextEditingController _options;
  var _targetKind = 'ruleset';
  var _mode = 'every'; // every | cron | once
  var _overlap = 'skip';
  var _catchUp = 'once';
  var _preview = '';
  var _busy = false;

  @override
  void initState() {
    super.initState();
    final e = widget.existing;
    _name = TextEditingController(text: e?.name ?? '');
    if (widget.presetPlugin != null) {
      _targetKind = 'connector';
      _target = TextEditingController(text: widget.presetPlugin);
    } else if (widget.presetRuleset != null) {
      _targetKind = 'ruleset';
      _target = TextEditingController(text: widget.presetRuleset);
    } else {
      _targetKind = e?.targetKind == 'connector' ? 'connector' : 'ruleset';
      _target = TextEditingController(text: e?.target ?? '');
    }
    _source = TextEditingController();
    _pipeline = TextEditingController();
    _ruleset = TextEditingController();
    _options = TextEditingController();
    _startAt = TextEditingController();
    _repeat = TextEditingController(
      text: (e?.runEvery.isNotEmpty == true && e!.runEvery != 'once')
          ? e.runEvery
          : '1h',
    );
    _cron = TextEditingController(text: e?.cron ?? '');
    _endsAt = TextEditingController();
    _for = TextEditingController();
    _overlap = e?.overlap.isNotEmpty == true ? e!.overlap : 'skip';
    _catchUp = e?.catchUp.isNotEmpty == true ? e!.catchUp : 'once';
    _tz = TextEditingController(
      text: e?.tz.isNotEmpty == true ? e!.tz : 'local',
    );
    if (e != null) {
      if (e.cron.isNotEmpty) {
        _mode = 'cron';
      } else if (e.runEvery == 'once') {
        _mode = 'once';
      } else {
        _mode = 'every';
      }
    }
  }

  @override
  void dispose() {
    _name.dispose();
    _target.dispose();
    _source.dispose();
    _startAt.dispose();
    _repeat.dispose();
    _cron.dispose();
    _endsAt.dispose();
    _for.dispose();
    _tz.dispose();
    _pipeline.dispose();
    _ruleset.dispose();
    _options.dispose();
    super.dispose();
  }

  Future<void> _save({bool pastNext = false}) async {
    final m = context.muninn;
    setState(() => _busy = true);
    try {
      final runEvery = switch (_mode) {
        'once' => 'once',
        'every' => _repeat.text.trim(),
        _ => null,
      };
      final cron = _mode == 'cron' ? _cron.text.trim() : null;
      final startAt = _startAt.text.trim().isEmpty
          ? null
          : _startAt.text.trim();
      final endsAt = _endsAt.text.trim().isEmpty ? null : _endsAt.text.trim();
      final for_ = _for.text.trim().isEmpty ? null : _for.text.trim();
      final tz = _tz.text.trim().isEmpty ? null : _tz.text.trim();
      final options = _options.text
          .split(',')
          .map((s) => s.trim())
          .where((s) => s.isNotEmpty)
          .toList();

      if (widget.existing == null) {
        // Past-start prompt for new schedules.
        if (startAt != null && !pastNext) {
          final check = await ref
              .read(apiProvider)
              .schedulePastStart(
                runEvery: runEvery,
                cron: cron,
                startAt: startAt,
                endsAt: endsAt,
                tz: tz,
              );
          if (check.startsWith('past:') && mounted) {
            final useNext = await _confirm(
              context,
              'Start is in the past',
              'Use the next interval (--past next-interval), or cancel and correct Start at?',
            );
            if (!useNext) {
              setState(() => _busy = false);
              return;
            }
            await _save(pastNext: true);
            return;
          }
        }
        await ref
            .read(apiProvider)
            .scheduleAdd(
              name: _name.text.trim(),
              runEvery: runEvery,
              ruleset: _targetKind == 'ruleset' ? _target.text.trim() : null,
              plugin: _targetKind == 'connector' ? _target.text.trim() : null,
              source: _targetKind == 'connector'
                  ? (_source.text.trim().isEmpty ? null : _source.text.trim())
                  : null,
              pipeline: _pipeline.text.trim().isEmpty
                  ? null
                  : _pipeline.text.trim(),
              connectorRuleset: _ruleset.text.trim().isEmpty
                  ? null
                  : _ruleset.text.trim(),
              options: options,
              cron: cron,
              startAt: startAt,
              endsAt: endsAt,
              for_: for_,
              tz: tz,
              pastNext: pastNext,
              overlap: _overlap,
              catchUp: _catchUp,
            );
      } else {
        await ref
            .read(apiProvider)
            .scheduleEdit(
              widget.existing!.id,
              name: _name.text.trim(),
              runEvery: runEvery,
              cron: cron,
              startAt: startAt,
              endsAt: endsAt,
              for_: for_,
              tz: tz,
              pastNext: pastNext,
              overlap: _overlap,
              catchUp: _catchUp,
            );
      }
      if (mounted) Navigator.pop(context, true);
    } catch (e) {
      final msg = '$e';
      if (msg.contains('past') && !pastNext && mounted) {
        final useNext = await _confirm(
          context,
          'Start is in the past',
          '$msg\n\nUse the next interval?',
        );
        if (useNext) {
          await _save(pastNext: true);
          return;
        }
      }
      if (mounted) _ok(context, msg, color: m.error);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    final editing = widget.existing != null;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text(
        editing ? 'Edit schedule' : 'New schedule',
        style: TextStyle(color: m.header),
      ),
      content: SizedBox(
        width: 460,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              TextField(
                key: const ValueKey('schedule-name'),
                controller: _name,
                decoration: const InputDecoration(labelText: 'Name'),
              ),
              if (!editing) ...[
                DropdownButtonFormField<String>(
                  key: const ValueKey('schedule-target-kind'),
                  initialValue: _targetKind,
                  items: const [
                    DropdownMenuItem(value: 'ruleset', child: Text('Rule set')),
                    DropdownMenuItem(
                      value: 'connector',
                      child: Text('Ingest connector'),
                    ),
                  ],
                  onChanged: (v) =>
                      setState(() => _targetKind = v ?? 'ruleset'),
                  decoration: const InputDecoration(labelText: 'Target'),
                ),
                TextField(
                  key: const ValueKey('schedule-target'),
                  controller: _target,
                  decoration: InputDecoration(
                    labelText: _targetKind == 'ruleset'
                        ? 'Rule set name'
                        : 'Connector (plugin) name',
                  ),
                ),
                if (_targetKind == 'connector') ...[
                  TextField(
                    key: const ValueKey('schedule-source'),
                    controller: _source,
                    decoration: const InputDecoration(labelText: 'Source name'),
                  ),
                  TextField(
                    controller: _pipeline,
                    decoration: const InputDecoration(
                      labelText: 'Pipeline (optional)',
                    ),
                  ),
                  TextField(
                    controller: _ruleset,
                    decoration: const InputDecoration(
                      labelText: 'Rule set the connector calls (optional)',
                    ),
                  ),
                  TextField(
                    controller: _options,
                    decoration: const InputDecoration(
                      labelText: 'Options key=value, comma-separated',
                    ),
                  ),
                ],
              ],
              DropdownButtonFormField<String>(
                key: const ValueKey('schedule-mode'),
                initialValue: _mode,
                items: const [
                  DropdownMenuItem(value: 'every', child: Text('Repeat every')),
                  DropdownMenuItem(value: 'cron', child: Text('Cron')),
                  DropdownMenuItem(value: 'once', child: Text('Once')),
                ],
                onChanged: (v) => setState(() => _mode = v ?? 'every'),
                decoration: const InputDecoration(labelText: 'Repeat'),
              ),
              if (_mode == 'every')
                TextField(
                  key: const ValueKey('schedule-every'),
                  controller: _repeat,
                  decoration: const InputDecoration(
                    labelText: 'Every (1h, 2 days, 1 month…)',
                  ),
                ),
              if (_mode == 'cron')
                TextField(
                  key: const ValueKey('schedule-cron'),
                  controller: _cron,
                  decoration: const InputDecoration(
                    labelText: 'Cron (minute hour day month weekday)',
                  ),
                ),
              TextField(
                key: const ValueKey('schedule-start'),
                controller: _startAt,
                decoration: const InputDecoration(
                  labelText: 'Start at (tomorrow 9am, next monday…)',
                ),
              ),
              if (_mode != 'once') ...[
                TextField(
                  key: const ValueKey('schedule-ends'),
                  controller: _endsAt,
                  decoration: const InputDecoration(
                    labelText: 'Ends at (optional)',
                  ),
                ),
                TextField(
                  key: const ValueKey('schedule-for'),
                  controller: _for,
                  decoration: const InputDecoration(
                    labelText: 'Or for (2 weeks)',
                  ),
                ),
              ] else
                const ListTile(
                  contentPadding: EdgeInsets.zero,
                  title: Text('Ends at'),
                  subtitle: Text('auto (once)'),
                ),
              DropdownButtonFormField<String>(
                initialValue: _overlap,
                items: const [
                  DropdownMenuItem(value: 'skip', child: Text('Overlap: skip')),
                  DropdownMenuItem(
                    value: 'queue',
                    child: Text('Overlap: queue'),
                  ),
                ],
                onChanged: (v) => setState(() => _overlap = v ?? 'skip'),
                decoration: const InputDecoration(labelText: 'Overlap'),
              ),
              DropdownButtonFormField<String>(
                initialValue: _catchUp,
                items: const [
                  DropdownMenuItem(
                    value: 'once',
                    child: Text('Catch up: once'),
                  ),
                  DropdownMenuItem(value: 'all', child: Text('Catch up: all')),
                  DropdownMenuItem(
                    value: 'none',
                    child: Text('Catch up: none'),
                  ),
                ],
                onChanged: (v) => setState(() => _catchUp = v ?? 'once'),
                decoration: const InputDecoration(labelText: 'Catch up'),
              ),
              TextField(
                controller: _tz,
                decoration: const InputDecoration(
                  labelText: 'Time zone (local or IANA)',
                ),
              ),
              if (_preview.isNotEmpty)
                Padding(
                  padding: const EdgeInsets.only(top: 8),
                  child: Text(
                    _preview,
                    key: const ValueKey('schedule-preview'),
                  ),
                ),
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          key: const ValueKey('schedule-preview-btn'),
          onPressed: _busy
              ? null
              : () async {
                  try {
                    final runEvery = switch (_mode) {
                      'once' => 'once',
                      'every' => _repeat.text.trim(),
                      _ => null,
                    };
                    final p = await ref
                        .read(apiProvider)
                        .scheduleParse(
                          runEvery: runEvery,
                          cron: _mode == 'cron' ? _cron.text.trim() : null,
                          startAt: _startAt.text.trim().isEmpty
                              ? null
                              : _startAt.text.trim(),
                          endsAt: _endsAt.text.trim().isEmpty
                              ? null
                              : _endsAt.text.trim(),
                          for_: _for.text.trim().isEmpty
                              ? null
                              : _for.text.trim(),
                          tz: _tz.text.trim().isEmpty ? null : _tz.text.trim(),
                        );
                    if (mounted) {
                      setState(() => _preview = p.lines.join('\n'));
                    }
                  } catch (e) {
                    if (mounted) setState(() => _preview = '$e');
                  }
                },
          child: const Text('Preview'),
        ),
        TextButton(
          onPressed: _busy ? null : () => Navigator.pop(context, false),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey('schedule-save'),
          onPressed: _busy ? null : () => _save(),
          child: Text(editing ? 'Save' : 'Create'),
        ),
      ],
    );
  }
}
