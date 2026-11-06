/// Net write dialogs (`mnnn net connect|invite|relay|share`). The node identity
/// and exchange keys stay in Rust; invite links hold no secret.
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

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
