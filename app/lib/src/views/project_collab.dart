/// Rules, Reviews and Notifications (`mnnn rule` / `review` / `notify` and the
/// TUI Rules / Reviews / Notifications windows). Exchange tokens and the node
/// key stay in Rust; the public review link is shown like a net invite.
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

Future<void> showNewRuleDialog(BuildContext context, WidgetRef ref) async {
  ref.read(sectionProvider.notifier).state = Section.projects;
  ref.read(projectPaneProvider.notifier).state = ProjectPane.rules;
  if (!ref.read(unlockedProvider)) return;
  final created = await showDialog<bool>(
    context: context,
    builder: (ctx) => const _RuleDialog(),
  );
  if (created == true) refreshProjectExtras(ref);
}

Future<void> showNewReviewDialog(BuildContext context, WidgetRef ref) async {
  ref.read(sectionProvider.notifier).state = Section.projects;
  ref.read(projectPaneProvider.notifier).state = ProjectPane.reviews;
  if (!ref.read(unlockedProvider)) return;
  final created = await showDialog<bool>(
    context: context,
    builder: (ctx) => const _ReviewRequestDialog(),
  );
  if (created == true) refreshProjectExtras(ref);
}

Future<void> showSendNotificationDialog(
  BuildContext context,
  WidgetRef ref,
) async {
  ref.read(sectionProvider.notifier).state = Section.projects;
  ref.read(projectPaneProvider.notifier).state = ProjectPane.notifications;
  if (!ref.read(unlockedProvider)) return;
  final sent = await showDialog<bool>(
    context: context,
    builder: (ctx) => const _SendNotificationDialog(),
  );
  if (sent == true) refreshProjectExtras(ref);
}

class RulesPane extends ConsumerWidget {
  const RulesPane({super.key});

  Future<void> _act(
    BuildContext context,
    WidgetRef ref,
    RuleInfo r,
    String v,
  ) async {
    final m = context.muninn;
    try {
      final api = ref.read(apiProvider);
      switch (v) {
        case 'open':
          final shown = await api.ruleShow(r.id);
          if (!context.mounted) return;
          await _showLines(context, r.name, shown);
        case 'edit':
          final ok = await showDialog<bool>(
            context: context,
            builder: (ctx) => _RuleDialog(existing: r),
          );
          if (ok == true) refreshProjectExtras(ref);
        case 'toggle':
          final msg = r.enabled
              ? await api.ruleDisable(r.id)
              : await api.ruleEnable(r.id);
          refreshProjectExtras(ref);
          if (context.mounted) _ok(context, msg);
        case 'test':
          final tested = await api.ruleTest(rule: r.id);
          if (!context.mounted) return;
          await _showLines(context, 'Test ${r.name}', tested);
        case 'dry':
          final planned = await api.ruleDryRun(rule: r.id);
          if (!context.mounted) return;
          await _showLines(context, 'Dry run ${r.name}', planned);
        case 'remove':
          if (!await _confirm(context, 'Remove rule', 'Remove ${r.name}?')) {
            return;
          }
          final msg = await api.ruleRemove(r.id);
          refreshProjectExtras(ref);
          if (context.mounted) _ok(context, msg);
      }
    } catch (e) {
      if (context.mounted) _ok(context, '$e', color: m.error);
    }
  }

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
            final all = items ?? const <RuleInfo>[];
            return ListView(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
              children: [
                Text(
                  'Rules act when something happens (`mnnn rule` / TUI Rules, R). On is action kinds or a schedule (every 6h, cron "0 3 * * *"). Then-actions run in order.',
                  style: TextStyle(color: m.muted),
                ),
                const SizedBox(height: 12),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    FilledButton.icon(
                      key: const ValueKey('rule-new'),
                      onPressed: () => showNewRuleDialog(context, ref),
                      icon: const Icon(Icons.add),
                      label: const Text('New rule…'),
                    ),
                    OutlinedButton(
                      onPressed: () async {
                        try {
                          final msg = await ref.read(apiProvider).ruleTick();
                          refreshProjectExtras(ref);
                          if (context.mounted) _ok(context, msg);
                        } catch (e) {
                          if (context.mounted) {
                            _ok(context, '$e', color: m.error);
                          }
                        }
                      },
                      child: const Text('Run due'),
                    ),
                    OutlinedButton(
                      onPressed: () async {
                        try {
                          final ev = await ref.read(apiProvider).ruleEvents();
                          if (context.mounted) {
                            await _showLines(context, 'Rule events', [
                              for (final e in ev)
                                '${e.kind}  ${e.about}  ${e.fields}',
                            ]);
                          }
                        } catch (e) {
                          if (context.mounted) {
                            _ok(context, '$e', color: m.error);
                          }
                        }
                      },
                      child: const Text('Events…'),
                    ),
                  ],
                ),
                const SizedBox(height: 12),
                if (all.isEmpty)
                  Text(
                    'No rules yet. Create one with an --on kind and a --then action.',
                    style: TextStyle(color: m.muted),
                  )
                else
                  for (final r in all)
                    ListTile(
                      key: ValueKey('rule-${r.id}'),
                      contentPadding: EdgeInsets.zero,
                      title: Text(r.name),
                      subtitle: Text(
                        [
                          r.enabled ? 'on' : 'off',
                          if (r.brief.isNotEmpty) r.brief,
                          if (r.home.isNotEmpty) r.home,
                          if (r.runs > 0)
                            '${r.runs} run${r.runs == 1 ? '' : 's'}',
                          if (r.lastStatus.isNotEmpty) 'last ${r.lastStatus}',
                        ].join(' · '),
                        style: TextStyle(color: m.muted),
                      ),
                      trailing: PopupMenuButton<String>(
                        onSelected: (v) => _act(context, ref, r, v),
                        itemBuilder: (_) => [
                          const PopupMenuItem(
                            value: 'open',
                            child: Text('Show'),
                          ),
                          const PopupMenuItem(
                            value: 'edit',
                            child: Text('Edit…'),
                          ),
                          PopupMenuItem(
                            value: 'toggle',
                            child: Text(r.enabled ? 'Disable' : 'Enable'),
                          ),
                          const PopupMenuItem(
                            value: 'test',
                            child: Text('Test'),
                          ),
                          const PopupMenuItem(
                            value: 'dry',
                            child: Text('Dry run'),
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

class _RuleDialog extends ConsumerStatefulWidget {
  const _RuleDialog({this.existing});
  final RuleInfo? existing;
  @override
  ConsumerState<_RuleDialog> createState() => _RuleDialogState();
}

class _RuleDialogState extends ConsumerState<_RuleDialog> {
  late final TextEditingController _name;
  late final TextEditingController _desc;
  late final TextEditingController _on;
  late final TextEditingController _when;
  late final TextEditingController _then;
  late final TextEditingController _home;
  late bool _related;

  @override
  void initState() {
    super.initState();
    final e = widget.existing;
    _name = TextEditingController(text: e?.name ?? '');
    _desc = TextEditingController(text: e?.description ?? '');
    _on = TextEditingController(text: e?.onText ?? '');
    _when = TextEditingController(text: e?.whenText ?? '');
    _then = TextEditingController(text: e?.thenText ?? '');
    _home = TextEditingController(
      text: e?.home == 'standalone' ? '' : (e?.home ?? ''),
    );
    _related = e?.related ?? false;
  }

  @override
  void dispose() {
    _name.dispose();
    _desc.dispose();
    _on.dispose();
    _when.dispose();
    _then.dispose();
    _home.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    try {
      if (widget.existing == null) {
        await ref
            .read(apiProvider)
            .ruleCreate(
              _name.text.trim(),
              description: _desc.text.trim(),
              on: _on.text.trim(),
              when: _when.text.trim(),
              then: _then.text.trim(),
              home: _home.text.trim(),
              related: _related,
            );
      } else {
        await ref
            .read(apiProvider)
            .ruleEdit(
              widget.existing!.id,
              name: _name.text.trim(),
              description: _desc.text.trim(),
              on: _on.text.trim(),
              when: _when.text.trim(),
              then: _then.text.trim(),
              home: _home.text.trim().isEmpty ? null : _home.text.trim(),
              related: _related,
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
        editing ? 'Edit rule' : 'New rule',
        style: TextStyle(color: m.header),
      ),
      content: SizedBox(
        width: 560,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              TextField(
                key: const ValueKey('rule-name'),
                controller: _name,
                decoration: const InputDecoration(labelText: 'Name'),
              ),
              TextField(
                controller: _desc,
                decoration: const InputDecoration(labelText: 'Description'),
              ),
              TextField(
                key: const ValueKey('rule-on'),
                controller: _on,
                decoration: const InputDecoration(
                  labelText: 'On',
                  hintText: 'import, journal.*, *   or every 6h   or cron "0 3 * * *"',
                ),
              ),
              TextField(
                controller: _when,
                decoration: const InputDecoration(
                  labelText: 'If (optional)',
                  hintText: 'inserted > 0',
                ),
              ),
              TextField(
                key: const ValueKey('rule-then'),
                controller: _then,
                maxLines: 3,
                decoration: const InputDecoration(
                  labelText: 'Then',
                  hintText: 'status Imported {inserted} bars',
                ),
              ),
              TextField(
                controller: _home,
                decoration: const InputDecoration(
                  labelText: 'Lives in (optional)',
                  hintText:
                      'project:NAME, journal:NAME, search:NAME or model:NAME',
                ),
              ),
              CheckboxListTile(
                contentPadding: EdgeInsets.zero,
                value: _related,
                onChanged: (v) => setState(() => _related = v ?? false),
                title: const Text('Only actions about where it lives'),
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
          key: const ValueKey('rule-save'),
          onPressed: _save,
          child: Text(editing ? 'Save' : 'Create'),
        ),
      ],
    );
  }
}

class ReviewsPane extends ConsumerStatefulWidget {
  const ReviewsPane({super.key});
  @override
  ConsumerState<ReviewsPane> createState() => _ReviewsPaneState();
}

class _ReviewsPaneState extends ConsumerState<ReviewsPane> {
  int _tab = 0; // 0 mine, 1 joined

  Future<void> _mine(BuildContext context, ReviewInfo r, String v) async {
    final m = context.muninn;
    try {
      final api = ref.read(apiProvider);
      switch (v) {
        case 'open':
          final d = await api.reviewShow(r.id);
          if (!context.mounted) return;
          await _showReview(context, d);
        case 'view':
          final viewed = await api.reviewView(r.id);
          if (!context.mounted) return;
          await _showLines(context, r.title, viewed);
        case 'comment':
          await _textAct(context, 'Comment', (t) => api.reviewComment(r.id, t));
        case 'suggest':
          await _suggest(context, r.id);
        case 'highlight':
          await _textAct(
            context,
            'Highlight at',
            (t) => api.reviewHighlight(r.id, at: t),
          );
        case 'vote':
          await _vote(context, r.id);
        case 'invite':
          final link = await api.reviewInvite(r.id);
          refreshProjectExtras(ref);
          if (context.mounted) await _showLines(context, 'Review link', [link]);
        case 'accept':
          final accepted = await api.reviewAccept(r.id, all: true);
          if (context.mounted) {
            _ok(context, accepted);
          }
        case 'reject':
          final rejected = await api.reviewReject(r.id, all: true);
          if (context.mounted) {
            _ok(context, rejected);
          }
        case 'merge':
          final merged = await api.reviewMerge(r.id);
          if (context.mounted) {
            _ok(context, merged);
          }
        case 'close':
          final closed = await api.reviewClose(r.id);
          refreshProjectExtras(ref);
          if (context.mounted) {
            _ok(context, closed);
          }
        case 'reopen':
          final opened = await api.reviewReopen(r.id);
          refreshProjectExtras(ref);
          if (context.mounted) {
            _ok(context, opened);
          }
        case 'revisions':
          final revs = await api.reviewRevisions(r.kind, r.name);
          if (!context.mounted) return;
          await _showLines(context, 'Revisions', revs);
        case 'remove':
          if (!await _confirm(
            context,
            'Remove review',
            'Delete ${r.title} and its thread? Revisions stay.',
          )) {
            return;
          }
          final msg = await api.reviewRemove(r.id);
          refreshProjectExtras(ref);
          if (context.mounted) _ok(context, msg);
      }
    } catch (e) {
      if (context.mounted) _ok(context, '$e', color: m.error);
    }
  }

  Future<void> _joined(BuildContext context, ReviewingInfo g, String v) async {
    final m = context.muninn;
    try {
      final api = ref.read(apiProvider);
      switch (v) {
        case 'open':
          final d = await api.reviewShow(g.id);
          if (!context.mounted) return;
          await _showReview(context, d);
        case 'sync':
          final msg = await api.reviewSync(g.id);
          refreshProjectExtras(ref);
          if (context.mounted) _ok(context, msg);
        case 'comment':
          await _textAct(context, 'Comment', (t) => api.reviewComment(g.id, t));
        case 'suggest':
          await _suggest(context, g.id);
        case 'vote':
          await _vote(context, g.id);
        case 'leave':
          if (!await _confirm(context, 'Leave review', 'Leave ${g.title}?')) {
            return;
          }
          final msg = await api.reviewLeave(g.id);
          refreshProjectExtras(ref);
          if (context.mounted) _ok(context, msg);
      }
    } catch (e) {
      if (context.mounted) _ok(context, '$e', color: m.error);
    }
  }

  Future<void> _textAct(
    BuildContext context,
    String title,
    Future<String> Function(String) run,
  ) async {
    final t = await _prompt(context, title);
    if (t == null || t.trim().isEmpty) return;
    final msg = await run(t.trim());
    refreshProjectExtras(ref);
    if (context.mounted) _ok(context, msg);
  }

  Future<void> _suggest(BuildContext context, String id) async {
    final to = await _prompt(context, 'Suggest replacement');
    if (to == null || to.trim().isEmpty) return;
    final msg = await ref.read(apiProvider).reviewSuggest(id, to.trim());
    refreshProjectExtras(ref);
    if (context.mounted) _ok(context, msg);
  }

  Future<void> _vote(BuildContext context, String id) async {
    final vote = await _prompt(context, 'Vote (approve, changes, reject)');
    if (vote == null || vote.trim().isEmpty) return;
    final msg = await ref.read(apiProvider).reviewVote(id, vote.trim());
    refreshProjectExtras(ref);
    if (context.mounted) _ok(context, msg);
  }

  Future<String?> _prompt(BuildContext context, String title) async {
    final c = TextEditingController();
    final m = context.muninn;
    final v = await showDialog<String>(
      context: context,
      builder: (ctx) => AlertDialog(
        backgroundColor: m.bg,
        title: Text(title, style: TextStyle(color: m.header)),
        content: TextField(controller: c, autofocus: true, maxLines: 4),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(ctx, c.text),
            child: const Text('OK'),
          ),
        ],
      ),
    );
    c.dispose();
    return v;
  }

  Future<void> _showReview(BuildContext context, ReviewDetail d) async {
    final m = context.muninn;
    await showDialog<void>(
      context: context,
      builder: (ctx) => AlertDialog(
        backgroundColor: m.bg,
        title: Text(d.review.title, style: TextStyle(color: m.header)),
        content: SizedBox(
          width: 560,
          height: 420,
          child: ListView(
            children: [
              Text(
                '${d.review.kind} ${d.review.name} · ${d.review.status} · may ${d.youMay}'
                '${d.peer.isEmpty ? '' : ' · on ${d.peer}'}',
                style: TextStyle(color: m.muted),
              ),
              if (d.note.isNotEmpty) Text(d.note),
              Text(
                'votes ${d.review.votesApprove} approve · ${d.review.votesChanges} changes · ${d.review.votesReject} reject',
                style: TextStyle(color: m.muted),
              ),
              if (d.reviewers.isNotEmpty) ...[
                const SizedBox(height: 8),
                Text('Reviewers', style: TextStyle(color: m.header)),
                for (final v in d.reviewers)
                  Text(
                    '${v.title} · ${v.status}',
                    style: TextStyle(color: m.muted),
                  ),
              ],
              const SizedBox(height: 8),
              Text('Thread', style: TextStyle(color: m.header)),
              if (d.items.isEmpty)
                Text('No items yet.', style: TextStyle(color: m.muted))
              else
                for (final i in d.items)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 4),
                    child: SelectableText(
                      i.line,
                      style: TextStyle(color: m.fg, fontFamily: 'monospace'),
                    ),
                  ),
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
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    final mine = ref.watch(reviewsProvider);
    final joined = ref.watch(reviewsJoinedProvider);
    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
      children: [
        Text(
          'Reviews of your journals, entries, projects, searches and records (`mnnn review` / TUI Reviews, V). Invite makes a public link; the exchange token stays in Rust.',
          style: TextStyle(color: m.muted),
        ),
        const SizedBox(height: 12),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            ChoiceChip(
              label: const Text('My reviews'),
              selected: _tab == 0,
              onSelected: (_) => setState(() => _tab = 0),
            ),
            ChoiceChip(
              label: const Text('Joined'),
              selected: _tab == 1,
              onSelected: (_) => setState(() => _tab = 1),
            ),
            FilledButton.icon(
              key: const ValueKey('review-new'),
              onPressed: () => showNewReviewDialog(context, ref),
              icon: const Icon(Icons.add),
              label: const Text('New review…'),
            ),
            if (_tab == 1)
              OutlinedButton(
                key: const ValueKey('review-join'),
                onPressed: () async {
                  final token = await _prompt(
                    context,
                    'Review link (mnnn-x1-…)',
                  );
                  if (token == null || token.trim().isEmpty) return;
                  try {
                    final msg = await ref
                        .read(apiProvider)
                        .reviewJoin(token.trim());
                    refreshProjectExtras(ref);
                    if (context.mounted) _ok(context, msg);
                  } catch (e) {
                    if (context.mounted) _ok(context, '$e', color: m.error);
                  }
                },
                child: const Text('Join…'),
              ),
          ],
        ),
        const SizedBox(height: 12),
        if (_tab == 0)
          mine.when(
            loading: () => const LinearProgressIndicator(),
            error: (e, _) => Text('$e', style: TextStyle(color: m.error)),
            data: (items) {
              final all = items ?? const <ReviewInfo>[];
              if (all.isEmpty) {
                return Text(
                  'No reviews yet. Ask for one of a journal, entry, project, search or record.',
                  style: TextStyle(color: m.muted),
                );
              }
              return Column(
                children: [
                  for (final r in all)
                    ListTile(
                      key: ValueKey('review-${r.id}'),
                      contentPadding: EdgeInsets.zero,
                      title: Text(r.title),
                      subtitle: Text(
                        '${r.kind} ${r.name} · ${r.status} · ${r.items} items · ${r.votesApprove}/${r.votesChanges}/${r.votesReject} votes',
                        style: TextStyle(color: m.muted),
                      ),
                      trailing: PopupMenuButton<String>(
                        onSelected: (v) => _mine(context, r, v),
                        itemBuilder: (_) => const [
                          PopupMenuItem(value: 'open', child: Text('Open')),
                          PopupMenuItem(value: 'view', child: Text('View')),
                          PopupMenuItem(
                            value: 'comment',
                            child: Text('Comment'),
                          ),
                          PopupMenuItem(
                            value: 'suggest',
                            child: Text('Suggest'),
                          ),
                          PopupMenuItem(
                            value: 'highlight',
                            child: Text('Highlight'),
                          ),
                          PopupMenuItem(value: 'vote', child: Text('Vote')),
                          PopupMenuItem(
                            value: 'invite',
                            child: Text('Invite…'),
                          ),
                          PopupMenuItem(
                            value: 'accept',
                            child: Text('Accept all'),
                          ),
                          PopupMenuItem(
                            value: 'reject',
                            child: Text('Reject all'),
                          ),
                          PopupMenuItem(value: 'merge', child: Text('Merge')),
                          PopupMenuItem(value: 'close', child: Text('Close')),
                          PopupMenuItem(value: 'reopen', child: Text('Reopen')),
                          PopupMenuItem(
                            value: 'revisions',
                            child: Text('Revisions'),
                          ),
                          PopupMenuItem(value: 'remove', child: Text('Remove')),
                        ],
                      ),
                    ),
                ],
              );
            },
          )
        else
          joined.when(
            loading: () => const LinearProgressIndicator(),
            error: (e, _) => Text('$e', style: TextStyle(color: m.error)),
            data: (items) {
              final all = items ?? const <ReviewingInfo>[];
              if (all.isEmpty) {
                return Text(
                  'You have not joined a review. Paste a review link with Join…',
                  style: TextStyle(color: m.muted),
                );
              }
              return Column(
                children: [
                  for (final g in all)
                    ListTile(
                      contentPadding: EdgeInsets.zero,
                      title: Text(g.title),
                      subtitle: Text(
                        '${g.kind} · ${g.status} · may ${g.allow} · at ${g.peer}',
                        style: TextStyle(color: m.muted),
                      ),
                      trailing: PopupMenuButton<String>(
                        onSelected: (v) => _joined(context, g, v),
                        itemBuilder: (_) => const [
                          PopupMenuItem(value: 'open', child: Text('Open')),
                          PopupMenuItem(value: 'sync', child: Text('Sync')),
                          PopupMenuItem(
                            value: 'comment',
                            child: Text('Comment'),
                          ),
                          PopupMenuItem(
                            value: 'suggest',
                            child: Text('Suggest'),
                          ),
                          PopupMenuItem(value: 'vote', child: Text('Vote')),
                          PopupMenuItem(value: 'leave', child: Text('Leave')),
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

class _ReviewRequestDialog extends ConsumerStatefulWidget {
  const _ReviewRequestDialog();
  @override
  ConsumerState<_ReviewRequestDialog> createState() =>
      _ReviewRequestDialogState();
}

class _ReviewRequestDialogState extends ConsumerState<_ReviewRequestDialog> {
  final _target = TextEditingController();
  final _title = TextEditingController();
  final _note = TextEditingController();
  final _allow = TextEditingController();
  String _kind = 'journal';

  @override
  void dispose() {
    _target.dispose();
    _title.dispose();
    _note.dispose();
    _allow.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    try {
      final allow = [
        for (final p in _allow.text.split(RegExp(r'[\s,]')))
          if (p.trim().isNotEmpty) p.trim(),
      ];
      await ref
          .read(apiProvider)
          .reviewRequest(
            _kind,
            _target.text.trim(),
            title: _title.text.trim(),
            note: _note.text.trim(),
            allow: allow,
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
      title: Text('New review', style: TextStyle(color: m.header)),
      content: SizedBox(
        width: 480,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              DropdownButtonFormField<String>(
                key: const ValueKey('review-kind'),
                initialValue: _kind,
                items: [
                  for (final k in [
                    'entry',
                    'journal',
                    'project',
                    'search',
                    'record',
                  ])
                    DropdownMenuItem(value: k, child: Text(k)),
                ],
                onChanged: (v) => setState(() => _kind = v ?? 'journal'),
                decoration: const InputDecoration(labelText: 'Kind'),
              ),
              TextField(
                key: const ValueKey('review-target'),
                controller: _target,
                decoration: const InputDecoration(
                  labelText: 'Name or id',
                  hintText: 'journal name, or MODEL/RECORD',
                ),
              ),
              TextField(
                controller: _title,
                decoration: const InputDecoration(
                  labelText: 'Title (optional)',
                ),
              ),
              TextField(
                controller: _note,
                decoration: const InputDecoration(
                  labelText: 'Note for reviewers',
                ),
              ),
              TextField(
                controller: _allow,
                decoration: const InputDecoration(
                  labelText: 'Allow (optional)',
                  hintText: 'view, comment, suggest, highlight, vote',
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
          key: const ValueKey('review-save'),
          onPressed: _save,
          child: const Text('Request'),
        ),
      ],
    );
  }
}

class NotificationsPane extends ConsumerWidget {
  const NotificationsPane({super.key});

  Future<void> _open(
    BuildContext context,
    WidgetRef ref,
    NotificationInfo n,
  ) async {
    try {
      final shown = await ref.read(apiProvider).notificationShow(n.id);
      refreshProjectExtras(ref);
      if (!context.mounted) return;
      await _showLines(context, shown.title, [
        '${shown.level} · ${shown.kind}',
        if (shown.linkLabel.isNotEmpty) 'about: ${shown.linkLabel}',
        if (shown.body.isNotEmpty) '',
        if (shown.body.isNotEmpty) shown.body,
      ]);
    } catch (e) {
      if (context.mounted) _ok(context, '$e', color: context.muninn.error);
    }
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final unreadOnly = ref.watch(notificationsUnreadOnlyProvider);
    return ref
        .watch(notificationsProvider)
        .when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Padding(
            padding: const EdgeInsets.all(16),
            child: Text('$e', style: TextStyle(color: m.error)),
          ),
          data: (items) {
            final all = items ?? const <NotificationInfo>[];
            return ListView(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
              children: [
                Text(
                  'What built-in events, rule sets (`then notify`) and broadcasts left for you (`mnnn notify` / TUI Notifications, !).',
                  style: TextStyle(color: m.muted),
                ),
                const SizedBox(height: 12),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    FilledButton.icon(
                      key: const ValueKey('notify-send'),
                      onPressed: () => showSendNotificationDialog(context, ref),
                      icon: const Icon(Icons.add_alert),
                      label: const Text('Send…'),
                    ),
                    OutlinedButton(
                      onPressed: () async {
                        try {
                          final msg = await ref
                              .read(apiProvider)
                              .notificationReadAll();
                          refreshProjectExtras(ref);
                          if (context.mounted) _ok(context, msg);
                        } catch (e) {
                          if (context.mounted) {
                            _ok(context, '$e', color: m.error);
                          }
                        }
                      },
                      child: const Text('Mark all read'),
                    ),
                    FilterChip(
                      label: const Text('Unread only'),
                      selected: unreadOnly,
                      onSelected: (v) =>
                          ref
                                  .read(
                                    notificationsUnreadOnlyProvider.notifier,
                                  )
                                  .state =
                              v,
                    ),
                    OutlinedButton(
                      onPressed: () async {
                        try {
                          final kinds = await ref
                              .read(apiProvider)
                              .notificationKinds();
                          if (!context.mounted) return;
                          await showDialog<void>(
                            context: context,
                            builder: (ctx) => AlertDialog(
                              backgroundColor: m.bg,
                              title: Text(
                                'Notification kinds',
                                style: TextStyle(color: m.header),
                              ),
                              content: SizedBox(
                                width: 460,
                                child: Column(
                                  mainAxisSize: MainAxisSize.min,
                                  children: [
                                    for (final k in kinds)
                                      ListTile(
                                        contentPadding: EdgeInsets.zero,
                                        title: Text(k.kind),
                                        subtitle: Text(
                                          k.about,
                                          style: TextStyle(color: m.muted),
                                        ),
                                        trailing: TextButton(
                                          onPressed: () async {
                                            try {
                                              final msg = k.muted
                                                  ? await ref
                                                        .read(apiProvider)
                                                        .notificationUnmute(
                                                          k.kind,
                                                        )
                                                  : await ref
                                                        .read(apiProvider)
                                                        .notificationMute(
                                                          k.kind,
                                                        );
                                              refreshProjectExtras(ref);
                                              if (context.mounted) {
                                                Navigator.pop(ctx);
                                                _ok(context, msg);
                                              }
                                            } catch (e) {
                                              if (context.mounted) {
                                                _ok(
                                                  context,
                                                  '$e',
                                                  color: m.error,
                                                );
                                              }
                                            }
                                          },
                                          child: Text(
                                            k.muted ? 'Unmute' : 'Mute',
                                          ),
                                        ),
                                      ),
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
                        } catch (e) {
                          if (context.mounted) {
                            _ok(context, '$e', color: m.error);
                          }
                        }
                      },
                      child: const Text('Kinds…'),
                    ),
                  ],
                ),
                const SizedBox(height: 12),
                if (all.isEmpty)
                  Text(
                    unreadOnly
                        ? 'No unread notifications.'
                        : 'No notifications yet.',
                    style: TextStyle(color: m.muted),
                  )
                else
                  for (final n in all)
                    ListTile(
                      key: ValueKey('notify-${n.id}'),
                      contentPadding: EdgeInsets.zero,
                      leading: Icon(
                        n.read ? Icons.notifications_none : Icons.notifications,
                        color: n.level == 'error'
                            ? m.error
                            : n.level == 'warn'
                            ? m.warning
                            : m.accent,
                      ),
                      title: Text(n.title),
                      subtitle: Text(
                        [
                          n.level,
                          n.kind,
                          if (n.linkLabel.isNotEmpty) n.linkLabel,
                          n.read ? 'read' : 'unread',
                        ].join(' · '),
                        style: TextStyle(color: m.muted),
                      ),
                      onTap: () => _open(context, ref, n),
                      trailing: PopupMenuButton<String>(
                        onSelected: (v) async {
                          try {
                            final api = ref.read(apiProvider);
                            final msg = v == 'unread'
                                ? await api.notificationUnread(n.id)
                                : await api.notificationRead(n.id);
                            refreshProjectExtras(ref);
                            if (context.mounted) _ok(context, msg);
                          } catch (e) {
                            if (context.mounted) {
                              _ok(context, '$e', color: m.error);
                            }
                          }
                        },
                        itemBuilder: (_) => [
                          if (n.read)
                            const PopupMenuItem(
                              value: 'unread',
                              child: Text('Mark unread'),
                            )
                          else
                            const PopupMenuItem(
                              value: 'read',
                              child: Text('Mark read'),
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

class _SendNotificationDialog extends ConsumerStatefulWidget {
  const _SendNotificationDialog();
  @override
  ConsumerState<_SendNotificationDialog> createState() =>
      _SendNotificationDialogState();
}

class _SendNotificationDialogState
    extends ConsumerState<_SendNotificationDialog> {
  final _title = TextEditingController();
  final _body = TextEditingController();
  final _link = TextEditingController();
  String _level = 'info';

  @override
  void dispose() {
    _title.dispose();
    _body.dispose();
    _link.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    try {
      await ref
          .read(apiProvider)
          .notificationSend(
            _title.text.trim(),
            body: _body.text.trim(),
            link: _link.text.trim().isEmpty ? null : _link.text.trim(),
            level: _level,
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
      title: Text('Send a notification', style: TextStyle(color: m.header)),
      content: SizedBox(
        width: 440,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              key: const ValueKey('notify-title'),
              controller: _title,
              decoration: const InputDecoration(labelText: 'Title'),
            ),
            TextField(
              controller: _body,
              maxLines: 3,
              decoration: const InputDecoration(labelText: 'Body'),
            ),
            TextField(
              controller: _link,
              decoration: const InputDecoration(
                labelText: 'Link (optional)',
                hintText: 'journal:NAME',
              ),
            ),
            DropdownButtonFormField<String>(
              initialValue: _level,
              items: [
                for (final l in ['info', 'warn', 'error'])
                  DropdownMenuItem(value: l, child: Text(l)),
              ],
              onChanged: (v) => setState(() => _level = v ?? 'info'),
              decoration: const InputDecoration(labelText: 'Level'),
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
          key: const ValueKey('notify-send-confirm'),
          onPressed: _save,
          child: const Text('Send'),
        ),
      ],
    );
  }
}
