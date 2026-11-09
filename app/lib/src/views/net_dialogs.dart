/// Net write dialogs (`mnnn net connect|invite|relay|share` plus settings leftovers:
/// exchange verify/label, remote fetch, broadcast and dark-shard entrust/check).
/// The node identity, exchange tokens and shard blobs stay in Rust; invite links
/// hold no secret.
library;

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../state.dart';
import '../theme/muninn_theme.dart';

void _snack(BuildContext context, String msg, {bool error = false}) {
  if (!context.mounted) {
    return;
  }
  final m = context.muninn;
  ScaffoldMessenger.of(context).showSnackBar(
    SnackBar(content: Text(msg), backgroundColor: error ? m.error : m.success),
  );
}

Future<void> _after(
  BuildContext context,
  WidgetRef ref,
  Future<String> work,
) async {
  try {
    final msg = await work;
    refreshNet(ref);
    if (!context.mounted) {
      return;
    }
    _snack(context, msg);
  } catch (e) {
    if (!context.mounted) {
      return;
    }
    _snack(context, '$e', error: true);
  }
}

Future<void> netStartNode(BuildContext context, WidgetRef ref) async {
  ref.read(sectionProvider.notifier).state = Section.net;
  ref.read(netPaneProvider.notifier).state = NetPane.node;
  if (!ref.read(unlockedProvider)) {
    return;
  }
  await _after(context, ref, ref.read(apiProvider).netStart());
}

Future<void> netStopNode(BuildContext context, WidgetRef ref) async {
  ref.read(sectionProvider.notifier).state = Section.net;
  ref.read(netPaneProvider.notifier).state = NetPane.node;
  if (!ref.read(unlockedProvider)) {
    return;
  }
  await _after(context, ref, ref.read(apiProvider).netStop());
}

Future<void> showNetConnectDialog(
  BuildContext context,
  WidgetRef ref, {
  String initial = '',
}) async {
  ref.read(sectionProvider.notifier).state = Section.net;
  if (!ref.read(unlockedProvider)) {
    return;
  }
  final result = await showDialog<(String, String, bool)>(
    context: context,
    builder: (ctx) => _ConnectDialog(initial: initial),
  );
  if (result == null || !context.mounted) {
    return;
  }
  final (target, label, hello) = result;
  try {
    final r = await ref
        .read(apiProvider)
        .netConnect(target, label: label.isEmpty ? null : label, hello: hello);
    refreshNet(ref);
    if (!context.mounted) {
      return;
    }
    _snack(context, '${r.status}: ${r.detail}');
  } catch (e) {
    if (!context.mounted) {
      return;
    }
    _snack(context, '$e', error: true);
  }
}

Future<void> showNetInviteDialog(BuildContext context, WidgetRef ref) async {
  ref.read(sectionProvider.notifier).state = Section.net;
  ref.read(netPaneProvider.notifier).state = NetPane.invites;
  if (!ref.read(unlockedProvider)) {
    return;
  }
  final label = await showDialog<String>(
    context: context,
    builder: (ctx) => const _LabelDialog(
      title: 'Invite',
      hint: 'Call the node that uses it this (optional)',
    ),
  );
  if (label == null || !context.mounted) {
    return;
  }
  try {
    final link = await ref
        .read(apiProvider)
        .netInvite(label: label.isEmpty ? null : label);
    refreshNet(ref);
    if (!context.mounted) {
      return;
    }
    await showNetLinkSheet(context, link);
  } catch (e) {
    if (!context.mounted) {
      return;
    }
    _snack(context, '$e', error: true);
  }
}

/// TUI Net ▸ Invite or connect…: empty field makes a link; otherwise connect.
Future<void> showNetInviteOrConnectDialog(
  BuildContext context,
  WidgetRef ref,
) async {
  ref.read(sectionProvider.notifier).state = Section.net;
  ref.read(netPaneProvider.notifier).state = NetPane.invites;
  if (!ref.read(unlockedProvider)) {
    return;
  }
  final target = await showDialog<String>(
    context: context,
    builder: (ctx) => const _LabelDialog(
      title: 'Invite or connect',
      hint: 'Invite link (Enter) or connect to (address, peer id)',
      keyName: 'net-invite-or-connect',
    ),
  );
  if (target == null || !context.mounted) {
    return;
  }
  if (target.trim().isEmpty) {
    try {
      final link = await ref.read(apiProvider).netInvite();
      refreshNet(ref);
      if (!context.mounted) {
        return;
      }
      await showNetLinkSheet(context, link);
    } catch (e) {
      if (!context.mounted) {
        return;
      }
      _snack(context, '$e', error: true);
    }
    return;
  }
  try {
    final r = await ref.read(apiProvider).netConnect(target.trim());
    refreshNet(ref);
    if (!context.mounted) {
      return;
    }
    _snack(context, '${r.status}: ${r.detail}');
  } catch (e) {
    if (!context.mounted) {
      return;
    }
    _snack(context, '$e', error: true);
  }
}

Future<void> showNetLinkSheet(BuildContext context, String link) async {
  final m = context.muninn;
  await showDialog<void>(
    context: context,
    builder: (ctx) => AlertDialog(
      backgroundColor: m.bg,
      title: Text('Invite link', style: TextStyle(color: m.header)),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            'Send it to them; when they use it the nodes connect. It holds no secret and expires in 7 days.',
            style: TextStyle(color: m.muted),
          ),
          const SizedBox(height: 12),
          SelectableText(
            link,
            key: const ValueKey('net-invite-link'),
            style: TextStyle(
              color: m.fg,
              fontFamily: 'monospace',
              fontSize: 13,
            ),
          ),
        ],
      ),
      actions: [
        TextButton(
          onPressed: () async {
            await Clipboard.setData(ClipboardData(text: link));
            if (ctx.mounted) Navigator.pop(ctx);
          },
          child: const Text('Copy'),
        ),
        TextButton(
          onPressed: () => Navigator.pop(ctx),
          child: const Text('Done'),
        ),
      ],
    ),
  );
}

Future<void> showNetRelayAddDialog(BuildContext context, WidgetRef ref) async {
  ref.read(netPaneProvider.notifier).state = NetPane.relays;
  if (!ref.read(unlockedProvider)) {
    return;
  }
  final addr = await showDialog<String>(
    context: context,
    builder: (ctx) => const _LabelDialog(
      title: 'Add relay',
      hint: 'Relay address (…/p2p/ID)',
      keyName: 'net-relay-addr',
    ),
  );
  if (addr == null || addr.trim().isEmpty || !context.mounted) {
    return;
  }
  await _after(context, ref, ref.read(apiProvider).netRelayAdd(addr.trim()));
}

Future<void> showNetShareDialog(BuildContext context, WidgetRef ref) async {
  ref.read(netPaneProvider.notifier).state = NetPane.shared;
  if (!ref.read(unlockedProvider)) {
    return;
  }
  final result = await showDialog<(String, String, String)>(
    context: context,
    builder: (ctx) => const _ShareDialog(),
  );
  if (result == null || !context.mounted) {
    return;
  }
  final (kind, item, withWhom) = result;
  final withPeers = withWhom.trim().isEmpty
      ? const <String>[]
      : [withWhom.trim()];
  await _after(
    context,
    ref,
    ref.read(apiProvider).netShare(kind, item, withPeers: withPeers),
  );
}

Future<bool> confirmNet(BuildContext context, String title, String body) async {
  final m = context.muninn;
  final ok = await showDialog<bool>(
    context: context,
    builder: (ctx) => AlertDialog(
      backgroundColor: m.bg,
      title: Text(title, style: TextStyle(color: m.header)),
      content: Text(body, style: TextStyle(color: m.fg)),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(ctx, false),
          child: const Text('Cancel'),
        ),
        TextButton(
          onPressed: () => Navigator.pop(ctx, true),
          child: Text('Confirm', style: TextStyle(color: m.error)),
        ),
      ],
    ),
  );
  return ok == true;
}

class _ConnectDialog extends StatefulWidget {
  const _ConnectDialog({this.initial = ''});
  final String initial;
  @override
  State<_ConnectDialog> createState() => _ConnectDialogState();
}

class _ConnectDialogState extends State<_ConnectDialog> {
  late final _target = TextEditingController(text: widget.initial);
  final _label = TextEditingController();
  bool _hello = false;

  @override
  void dispose() {
    _target.dispose();
    _label.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text('Connect', style: TextStyle(color: m.header)),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              'Address (…/p2p/ID), a peer id, a known name or an invite link. Keys are exchanged automatically.',
              style: TextStyle(color: m.muted),
            ),
            const SizedBox(height: 12),
            TextField(
              key: const ValueKey('net-connect-target'),
              controller: _target,
              autofocus: true,
              decoration: const InputDecoration(labelText: 'Connect to'),
            ),
            TextField(
              controller: _label,
              decoration: const InputDecoration(
                labelText: 'Call it this (optional)',
              ),
            ),
            CheckboxListTile(
              contentPadding: EdgeInsets.zero,
              value: _hello,
              onChanged: (v) => setState(() => _hello = v ?? false),
              title: const Text('Hello only (no key exchange)'),
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
          key: const ValueKey('net-connect-confirm'),
          onPressed: () => Navigator.pop(context, (
            _target.text.trim(),
            _label.text.trim(),
            _hello,
          )),
          child: const Text('Connect'),
        ),
      ],
    );
  }
}

class _LabelDialog extends StatefulWidget {
  const _LabelDialog({required this.title, required this.hint, this.keyName});
  final String title, hint;
  final String? keyName;
  @override
  State<_LabelDialog> createState() => _LabelDialogState();
}

class _LabelDialogState extends State<_LabelDialog> {
  final _ctl = TextEditingController();
  @override
  void dispose() {
    _ctl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text(widget.title, style: TextStyle(color: m.header)),
      content: TextField(
        key: widget.keyName == null ? null : ValueKey<String>(widget.keyName!),
        controller: _ctl,
        autofocus: true,
        decoration: InputDecoration(labelText: widget.hint),
        onSubmitted: (v) => Navigator.pop(context, v),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey('net-dialog-confirm'),
          onPressed: () => Navigator.pop(context, _ctl.text),
          child: const Text('OK'),
        ),
      ],
    );
  }
}

class _ShareDialog extends StatefulWidget {
  const _ShareDialog();
  @override
  State<_ShareDialog> createState() => _ShareDialogState();
}

class _ShareDialogState extends State<_ShareDialog> {
  String _kind = 'project';
  final _item = TextEditingController();
  final _with = TextEditingController();
  static const _kinds = [
    'project',
    'model',
    'search',
    'journal',
    'report',
    'code',
    'space',
    'pipeline',
  ];

  @override
  void dispose() {
    _item.dispose();
    _with.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text('Share', style: TextStyle(color: m.header)),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              'Share a model, search, journal, project, report, code space or pipeline. Leave “with” empty to share with every exchange.',
              style: TextStyle(color: m.muted),
            ),
            const SizedBox(height: 12),
            DropdownButtonFormField<String>(
              key: const ValueKey('net-share-kind'),
              initialValue: _kind,
              items: [
                for (final k in _kinds)
                  DropdownMenuItem(value: k, child: Text(k)),
              ],
              onChanged: (v) => setState(() => _kind = v ?? 'project'),
              decoration: const InputDecoration(labelText: 'Kind'),
            ),
            TextField(
              key: const ValueKey('net-share-item'),
              controller: _item,
              decoration: const InputDecoration(labelText: 'Name or id'),
            ),
            TextField(
              controller: _with,
              decoration: const InputDecoration(
                labelText: 'With exchange or peer (optional)',
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
          key: const ValueKey('net-share-confirm'),
          onPressed: () => Navigator.pop(context, (
            _kind,
            _item.text.trim(),
            _with.text.trim(),
          )),
          child: const Text('Share'),
        ),
      ],
    );
  }
}

Future<void> showNetExchangeLabelDialog(
  BuildContext context,
  WidgetRef ref,
  NetExchangeInfo x,
) async {
  final label = await showDialog<String>(
    context: context,
    builder: (ctx) => const _LabelDialog(
      title: 'Label exchange',
      hint: 'Label',
      keyName: 'net-exchange-label',
    ),
  );
  if (label == null || !context.mounted) {
    return;
  }
  await _after(
    context,
    ref,
    ref.read(apiProvider).netExchangeLabel(x.id, label.trim()),
  );
}

Future<void> showNetExchangeVerifyDialog(
  BuildContext context,
  WidgetRef ref,
  NetExchangeInfo x,
) async {
  final code = await showDialog<String>(
    context: context,
    builder: (ctx) => const _LabelDialog(
      title: 'Verify exchange',
      hint: 'Short code (read aloud)',
      keyName: 'net-exchange-verify',
    ),
  );
  if (code == null || code.trim().isEmpty || !context.mounted) {
    return;
  }
  await _after(
    context,
    ref,
    ref.read(apiProvider).netExchangeVerify(x.id, code.trim()),
  );
}

Future<void> showNetRemoteListDialog(
  BuildContext context,
  WidgetRef ref,
  NetExchangeInfo x,
) async {
  try {
    final items = await ref.read(apiProvider).netRemoteList(x.id);
    if (!context.mounted) {
      return;
    }
    await showDialog<void>(
      context: context,
      builder: (ctx) => _RemoteListDialog(exchange: x, items: items),
    );
  } catch (e) {
    if (context.mounted) {
      _snack(context, '$e', error: true);
    }
  }
}

class _RemoteListDialog extends ConsumerWidget {
  const _RemoteListDialog({required this.exchange, required this.items});
  final NetExchangeInfo exchange;
  final List<NetRemoteItem> items;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text(
        'Shared by ${exchange.title.isEmpty ? exchange.shortId : exchange.title}',
        style: TextStyle(color: m.header),
      ),
      content: SizedBox(
        width: 460,
        height: 360,
        child: items.isEmpty
            ? Text(
                'This peer is not sharing anything with you.',
                style: TextStyle(color: m.muted),
              )
            : ListView(
                children: [
                  for (final it in items)
                    ListTile(
                      contentPadding: EdgeInsets.zero,
                      title: Text('${it.kind} ${it.name}'),
                      subtitle: it.about.isEmpty
                          ? null
                          : Text(it.about, style: TextStyle(color: m.muted)),
                      trailing: Wrap(
                        children: [
                          TextButton(
                            onPressed: () async {
                              try {
                                final msg = await ref
                                    .read(apiProvider)
                                    .netFetch(exchange.id, it.id, import: true);
                                refreshNet(ref);
                                if (context.mounted) {
                                  Navigator.pop(context);
                                  _snack(context, msg);
                                }
                              } catch (e) {
                                if (context.mounted) {
                                  _snack(context, '$e', error: true);
                                }
                              }
                            },
                            child: const Text('Import'),
                          ),
                          TextButton(
                            onPressed: () async {
                              final path = await FilePicker.platform.saveFile(
                                fileName: it.name,
                              );
                              if (path == null || !context.mounted) {
                                return;
                              }
                              try {
                                final msg = await ref
                                    .read(apiProvider)
                                    .netFetch(exchange.id, it.id, path: path);
                                if (context.mounted) {
                                  Navigator.pop(context);
                                  _snack(context, msg);
                                }
                              } catch (e) {
                                if (context.mounted) {
                                  _snack(context, '$e', error: true);
                                }
                              }
                            },
                            child: const Text('Save…'),
                          ),
                        ],
                      ),
                    ),
                ],
              ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Close'),
        ),
      ],
    );
  }
}

Future<void> showNetBroadcastDialog(BuildContext context, WidgetRef ref) async {
  ref.read(sectionProvider.notifier).state = Section.net;
  if (!ref.read(unlockedProvider)) {
    return;
  }
  List<OrgInfo> orgs = const [];
  try {
    orgs = (await ref.read(orgOverviewProvider.future))?.mine ?? const [];
  } catch (_) {}
  if (!context.mounted) {
    return;
  }
  final result = await showDialog<(String, String, String)>(
    context: context,
    builder: (ctx) => _BroadcastDialog(orgs: orgs),
  );
  if (result == null || !context.mounted) {
    return;
  }
  final (topic, text, to) = result;
  await _after(
    context,
    ref,
    ref.read(apiProvider).netBroadcast(topic, text, to: to),
  );
}

class _BroadcastDialog extends StatefulWidget {
  const _BroadcastDialog({this.orgs = const []});
  final List<OrgInfo> orgs;
  @override
  State<_BroadcastDialog> createState() => _BroadcastDialogState();
}

class _BroadcastDialogState extends State<_BroadcastDialog> {
  final _topic = TextEditingController();
  final _text = TextEditingController();
  final _orgSlug = TextEditingController();
  String _to = 'nodes';

  @override
  void dispose() {
    _topic.dispose();
    _text.dispose();
    _orgSlug.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text('Broadcast', style: TextStyle(color: m.header)),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              'Send a short topic and text to linked nodes, your connections, everyone, or an org. Arrives as a net.broadcast action.',
              style: TextStyle(color: m.muted),
            ),
            TextField(
              key: const ValueKey('net-broadcast-topic'),
              controller: _topic,
              decoration: const InputDecoration(labelText: 'Topic'),
            ),
            TextField(
              key: const ValueKey('net-broadcast-text'),
              controller: _text,
              maxLines: 4,
              decoration: const InputDecoration(labelText: 'Message'),
            ),
            DropdownButtonFormField<String>(
              key: const ValueKey('net-broadcast-to'),
              initialValue: _to,
              items: [
                const DropdownMenuItem(value: 'nodes', child: Text('nodes')),
                const DropdownMenuItem(
                  value: 'connections',
                  child: Text('connections'),
                ),
                const DropdownMenuItem(value: 'all', child: Text('all')),
                for (final o in widget.orgs)
                  DropdownMenuItem(
                    value: 'org:${o.slug}',
                    child: Text('${o.name} (org:${o.slug})'),
                  ),
              ],
              onChanged: (v) => setState(() => _to = v ?? 'nodes'),
              decoration: const InputDecoration(labelText: 'To'),
            ),
            TextField(
              key: const ValueKey('net-broadcast-org'),
              controller: _orgSlug,
              decoration: const InputDecoration(
                labelText: 'Or org slug (org:SLUG)',
                hintText: 'lab',
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
          key: const ValueKey('net-broadcast-confirm'),
          onPressed: () {
            final slug = _orgSlug.text.trim();
            final to = slug.isEmpty
                ? _to
                : (slug.startsWith('org:') ? slug : 'org:$slug');
            Navigator.pop(context, (_topic.text.trim(), _text.text.trim(), to));
          },
          child: const Text('Send'),
        ),
      ],
    );
  }
}

Future<void> showNetShardEntrustDialog(
  BuildContext context,
  WidgetRef ref,
) async {
  if (!ref.read(unlockedProvider)) {
    return;
  }
  final result = await showDialog<_EntrustDraft>(
    context: context,
    builder: (ctx) => const _EntrustDialog(),
  );
  if (result == null || !context.mounted) {
    return;
  }
  await _after(
    context,
    ref,
    ref
        .read(apiProvider)
        .netShardEntrust(
          to: result.to,
          keyFile: result.keyFile,
          shardPassword: result.shardPassword,
          filePassword: result.filePassword,
          threshold: result.threshold,
        ),
  );
}

class _EntrustDraft {
  const _EntrustDraft({
    required this.to,
    required this.keyFile,
    required this.shardPassword,
    this.filePassword,
    this.threshold = 0,
  });
  final List<String> to;
  final String keyFile, shardPassword;
  final String? filePassword;
  final int threshold;
}

class _EntrustDialog extends ConsumerStatefulWidget {
  const _EntrustDialog();
  @override
  ConsumerState<_EntrustDialog> createState() => _EntrustDialogState();
}

class _EntrustDialogState extends ConsumerState<_EntrustDialog> {
  String? _keyFile;
  final _filePass = TextEditingController();
  final _shardPass = TextEditingController();
  final _repeat = TextEditingController();
  final _threshold = TextEditingController(text: '0');
  final _picked = <String>{};

  @override
  void dispose() {
    _filePass.dispose();
    _shardPass.dispose();
    _repeat.dispose();
    _threshold.dispose();
    super.dispose();
  }

  Future<void> _pick() async {
    final r = await FilePicker.platform.pickFiles();
    final path = r?.files.single.path;
    if (path != null) {
      setState(() => _keyFile = path);
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    final xs =
        ref.watch(netExchangesProvider).value ?? const <NetExchangeInfo>[];
    final active = [
      for (final x in xs)
        if (x.active) x,
    ];
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text('Entrust dark shards', style: TextStyle(color: m.header)),
      content: SizedBox(
        width: 460,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                'Split this database’s key across at least two connected nodes. The key is read from a file (the same files Keys ▸ Export writes); it never appears in the app. You will need the shard password again to restore.',
                style: TextStyle(color: m.muted),
              ),
              const SizedBox(height: 12),
              OutlinedButton(
                key: const ValueKey('net-entrust-pick'),
                onPressed: _pick,
                child: Text(_keyFile == null ? 'Choose key file…' : _keyFile!),
              ),
              TextField(
                controller: _filePass,
                obscureText: true,
                decoration: const InputDecoration(
                  labelText: 'Key file password (if encrypted)',
                ),
              ),
              TextField(
                key: const ValueKey('net-entrust-password'),
                controller: _shardPass,
                obscureText: true,
                decoration: const InputDecoration(labelText: 'Shard password'),
              ),
              TextField(
                controller: _repeat,
                obscureText: true,
                decoration: const InputDecoration(
                  labelText: 'Repeat shard password',
                ),
              ),
              TextField(
                controller: _threshold,
                keyboardType: TextInputType.number,
                decoration: const InputDecoration(
                  labelText: 'Threshold (0 = half, at least 2)',
                ),
              ),
              const SizedBox(height: 8),
              Text(
                'Holders (pick at least two)',
                style: TextStyle(color: m.muted),
              ),
              if (active.isEmpty)
                Text(
                  'Connect to at least two nodes first.',
                  style: TextStyle(color: m.muted),
                )
              else
                Wrap(
                  spacing: 8,
                  children: [
                    for (final x in active)
                      FilterChip(
                        label: Text(x.title.isEmpty ? x.shortId : x.title),
                        selected: _picked.contains(x.id),
                        onSelected: (on) => setState(() {
                          if (on) {
                            _picked.add(x.id);
                          } else {
                            _picked.remove(x.id);
                          }
                        }),
                      ),
                  ],
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
          key: const ValueKey('net-entrust-confirm'),
          onPressed: () {
            if (_keyFile == null) {
              _snack(context, 'Choose the key file to split', error: true);
              return;
            }
            if (_shardPass.text.isEmpty || _shardPass.text != _repeat.text) {
              _snack(context, 'Type the shard password twice', error: true);
              return;
            }
            if (_picked.length < 2) {
              _snack(context, 'Pick at least two connected nodes', error: true);
              return;
            }
            Navigator.pop(
              context,
              _EntrustDraft(
                to: _picked.toList(),
                keyFile: _keyFile!,
                shardPassword: _shardPass.text,
                filePassword: _filePass.text.trim().isEmpty
                    ? null
                    : _filePass.text,
                threshold: int.tryParse(_threshold.text.trim()) ?? 0,
              ),
            );
          },
          child: const Text('Entrust'),
        ),
      ],
    );
  }
}

Future<void> showNetShardCheckDialog(
  BuildContext context,
  WidgetRef ref,
) async {
  if (!ref.read(unlockedProvider)) {
    return;
  }
  final pw = await showDialog<String>(
    context: context,
    builder: (ctx) {
      final ctl = TextEditingController();
      final m = ctx.muninn;
      return AlertDialog(
        backgroundColor: m.bg,
        title: Text('Check restore', style: TextStyle(color: m.header)),
        content: TextField(
          key: const ValueKey('net-shard-check-password'),
          controller: ctl,
          obscureText: true,
          autofocus: true,
          decoration: const InputDecoration(labelText: 'Shard password'),
          onSubmitted: (v) => Navigator.pop(ctx, v),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: const Text('Cancel'),
          ),
          FilledButton(
            key: const ValueKey('net-shard-check-confirm'),
            onPressed: () => Navigator.pop(ctx, ctl.text),
            child: const Text('Check'),
          ),
        ],
      );
    },
  );
  if (pw == null || pw.isEmpty || !context.mounted) {
    return;
  }
  await _after(context, ref, ref.read(apiProvider).netShardCheck(pw));
}

class _ExchangeBackupFields {
  const _ExchangeBackupFields({required this.path, required this.password});
  final String path;
  final String password;
}

Future<void> showNetExchangeExportDialog(
  BuildContext context,
  WidgetRef ref,
) async {
  ref.read(sectionProvider.notifier).state = Section.net;
  ref.read(netPaneProvider.notifier).state = NetPane.exchanges;
  if (!ref.read(unlockedProvider)) {
    return;
  }
  final result = await showDialog<_ExchangeBackupFields>(
    context: context,
    builder: (ctx) => const _ExchangeExportDialog(),
  );
  if (result == null || !context.mounted) {
    return;
  }
  await _after(
    context,
    ref,
    ref
        .read(apiProvider)
        .netExchangeExport(out: result.path, password: result.password),
  );
}

class _ExchangeExportDialog extends StatefulWidget {
  const _ExchangeExportDialog();
  @override
  State<_ExchangeExportDialog> createState() => _ExchangeExportDialogState();
}

class _ExchangeExportDialogState extends State<_ExchangeExportDialog> {
  final _out = TextEditingController();
  final _password = TextEditingController();

  @override
  void dispose() {
    _out.dispose();
    _password.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text('Export exchanges', style: TextStyle(color: m.header)),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              'Writes the node identity, exchange keys and known peers to an age-encrypted file. The password seals the file; bytes never enter Dart.',
              style: TextStyle(color: m.muted),
            ),
            TextField(
              key: const ValueKey('net-exchange-export-out'),
              controller: _out,
              decoration: const InputDecoration(
                labelText: 'Output file',
                hintText: 'muninn-exchanges.age',
              ),
            ),
            Align(
              alignment: Alignment.centerLeft,
              child: TextButton(
                onPressed: () async {
                  final path = await FilePicker.platform.saveFile(
                    fileName: 'muninn-exchanges.age',
                  );
                  if (path != null) {
                    _out.text = path;
                  }
                },
                child: const Text('Choose file…'),
              ),
            ),
            TextField(
              key: const ValueKey('net-exchange-export-password'),
              controller: _password,
              obscureText: true,
              decoration: const InputDecoration(labelText: 'Export password'),
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
          key: const ValueKey('net-exchange-export-confirm'),
          onPressed: () => Navigator.pop(
            context,
            _ExchangeBackupFields(
              path: _out.text.trim(),
              password: _password.text,
            ),
          ),
          child: const Text('Export'),
        ),
      ],
    );
  }
}

Future<void> showNetExchangeRestoreDialog(
  BuildContext context,
  WidgetRef ref,
) async {
  ref.read(sectionProvider.notifier).state = Section.net;
  ref.read(netPaneProvider.notifier).state = NetPane.exchanges;
  if (!ref.read(unlockedProvider)) {
    return;
  }
  final result = await showDialog<_ExchangeBackupFields>(
    context: context,
    builder: (ctx) => const _ExchangeRestoreDialog(),
  );
  if (result == null || !context.mounted) {
    return;
  }
  try {
    final api = ref.read(apiProvider);
    final peek = await api.netExchangeRestorePeek(
      file: result.path,
      password: result.password,
    );
    var confirm = !peek.identityReplaced;
    if (peek.identityReplaced && context.mounted) {
      confirm = await confirmNet(
        context,
        'Replace node identity',
        "This database's node identity becomes ${peek.peerId} (from the backup). Exchanges made with the current identity stop working. Continue?",
      );
    }
    if (!confirm || !context.mounted) {
      return;
    }
    await _after(
      context,
      ref,
      api.netExchangeRestore(
        file: result.path,
        password: result.password,
        confirmReplace: peek.identityReplaced,
      ),
    );
  } catch (e) {
    if (context.mounted) {
      _snack(context, '$e', error: true);
    }
  }
}

class _ExchangeRestoreDialog extends StatefulWidget {
  const _ExchangeRestoreDialog();
  @override
  State<_ExchangeRestoreDialog> createState() => _ExchangeRestoreDialogState();
}

class _ExchangeRestoreDialogState extends State<_ExchangeRestoreDialog> {
  final _file = TextEditingController();
  final _password = TextEditingController();

  @override
  void dispose() {
    _file.dispose();
    _password.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text('Restore exchanges', style: TextStyle(color: m.header)),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              'Restores the node identity and key exchanges from a password-sealed backup. Decrypts in Rust; keys never enter Dart. If this database already has a different identity, you will be asked to confirm.',
              style: TextStyle(color: m.muted),
            ),
            TextField(
              key: const ValueKey('net-exchange-restore-file'),
              controller: _file,
              decoration: const InputDecoration(labelText: 'Backup file'),
            ),
            Align(
              alignment: Alignment.centerLeft,
              child: TextButton(
                onPressed: () async {
                  final r = await FilePicker.platform.pickFiles();
                  final path = r?.files.single.path;
                  if (path != null) {
                    _file.text = path;
                  }
                },
                child: const Text('Choose file…'),
              ),
            ),
            TextField(
              key: const ValueKey('net-exchange-restore-password'),
              controller: _password,
              obscureText: true,
              decoration: const InputDecoration(labelText: 'Export password'),
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
          key: const ValueKey('net-exchange-restore-confirm'),
          onPressed: () => Navigator.pop(
            context,
            _ExchangeBackupFields(
              path: _file.text.trim(),
              password: _password.text,
            ),
          ),
          child: const Text('Restore'),
        ),
      ],
    );
  }
}
