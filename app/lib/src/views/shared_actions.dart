/// Actions the phone and desktop apps share: MCP add, plugin install,
/// announcing a site, raw exchange tokens, shard-backup restore, live
/// send/host/join, and piping a code result. Tokens are shown once.
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

Future<void> _once(
  BuildContext context,
  String title,
  List<String> lines,
) async {
  final text = lines.where((l) => l.isNotEmpty).join('\n');
  if (text.isEmpty) return;
  await showDialog<void>(
    context: context,
    builder: (ctx) => AlertDialog(
      title: Text(title),
      content: SelectableText(text, key: const ValueKey('once-secret')),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(ctx),
          child: const Text('Done'),
        ),
      ],
    ),
  );
}

Future<void> showMcpAddServerDialog(BuildContext context, WidgetRef ref) async {
  final msg = await showDialog<String>(
    context: context,
    builder: (_) => const _McpServerDialog(),
  );
  if (msg == null) return;
  refreshNet(ref);
  if (context.mounted) _ok(context, msg);
}

Future<void> showMcpAddClientDialog(BuildContext context, WidgetRef ref) async {
  final added = await showDialog<McpClientAdded>(
    context: context,
    builder: (_) => const _McpClientDialog(),
  );
  if (added == null || !context.mounted) return;
  refreshNet(ref);
  await _once(context, 'Client ${added.name}', [
    'role ${added.role}',
    'This token is shown once. It is not stored in the app:',
    added.token,
  ]);
}

Future<void> showPluginInstallDialog(
  BuildContext context,
  WidgetRef ref,
) async {
  final msg = await showDialog<String>(
    context: context,
    builder: (_) => const _PluginInstallDialog(),
  );
  if (msg == null) return;
  refreshProjectExtras(ref);
  if (context.mounted) _ok(context, msg);
}

Future<void> showCodePipeDialog(
  BuildContext context,
  WidgetRef ref,
  CodeSpaceInfo space,
) async {
  List<PipeTargetInfo> targets;
  try {
    targets = await ref.read(apiProvider).codePipeTargets(space.id);
  } catch (e) {
    if (context.mounted) _ok(context, '$e', color: context.muninn.error);
    return;
  }
  if (!context.mounted) return;
  final picked = await showDialog<PipeTargetInfo>(
    context: context,
    builder: (ctx) => AlertDialog(
      title: Text('Pipe ${space.name}'),
      content: SizedBox(
        width: 420,
        child: ListView(
          shrinkWrap: true,
          children: [
            for (final t in targets)
              ListTile(
                key: ValueKey('pipe-${t.kind}-${t.name}'),
                title: Text('${t.kind}: ${t.name}'),
                onTap: () => Navigator.pop(ctx, t),
              ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(ctx),
          child: const Text('Cancel'),
        ),
      ],
    ),
  );
  if (picked == null || !context.mounted) return;
  try {
    final msg = await ref
        .read(apiProvider)
        .codePipeTo(space.id, picked.kind, picked.name);
    refreshProjectExtras(ref);
    if (context.mounted) _ok(context, msg);
  } catch (e) {
    if (context.mounted) _ok(context, '$e', color: context.muninn.error);
  }
}

Future<void> showOrgAnnounceDialog(BuildContext context, WidgetRef ref) async {
  final lines = await showDialog<List<String>>(
    context: context,
    builder: (_) => const _AnnounceDialog(),
  );
  if (lines == null || !context.mounted) return;
  await _once(context, 'Announce', lines);
}

Future<void> showAnnouncedSitesDialog(
  BuildContext context,
  WidgetRef ref,
) async {
  try {
    final sites = await ref.read(apiProvider).orgAnnounced();
    if (!context.mounted) return;
    await showDialog<void>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Announced sites'),
        content: SizedBox(
          width: 420,
          child: sites.isEmpty
              ? const Text('No sites have been announced.')
              : ListView(
                  shrinkWrap: true,
                  children: [
                    for (final s in sites)
                      ListTile(
                        key: ValueKey('announced-${s.slug}'),
                        title: Text(s.name.isEmpty ? s.slug : s.name),
                        subtitle: Text(
                          '${s.peer} · ${s.open ? 'open' : 'closed'}${s.urls.isEmpty ? '' : ' · ${s.urls.join(', ')}'}',
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
    if (context.mounted) _ok(context, '$e', color: context.muninn.error);
  }
}

Future<void> showExchangeOfferDialog(
  BuildContext context,
  WidgetRef ref,
) async {
  final once = await showDialog<ExchangeOnce>(
    context: context,
    builder: (_) => const _OfferDialog(),
  );
  if (once == null || !context.mounted) return;
  refreshNet(ref);
  _ok(context, once.summary);
  await _once(context, 'Offer token', [once.token]);
}

Future<void> showExchangeAcceptDialog(
  BuildContext context,
  WidgetRef ref,
) async {
  final once = await showDialog<ExchangeOnce>(
    context: context,
    builder: (_) => const _AcceptDialog(),
  );
  if (once == null || !context.mounted) return;
  refreshNet(ref);
  _ok(context, once.summary);
  await _once(context, 'Answer', [
    if (once.shortCode.isNotEmpty) 'short code: ${once.shortCode}',
    once.answer,
  ]);
}

Future<void> showShardBackupRestoreDialog(
  BuildContext context,
  WidgetRef ref,
) async {
  final msg = await showDialog<String>(
    context: context,
    builder: (_) => const _ShardRestoreDialog(),
  );
  if (msg != null && context.mounted) _ok(context, msg);
}

Future<void> showCollabSendDialog(BuildContext context, WidgetRef ref) async {
  final msg = await showDialog<String>(
    context: context,
    builder: (_) => const _CollabDialog(mode: _Collab.send),
  );
  if (msg == null) return;
  refreshNet(ref);
  if (context.mounted) _ok(context, msg);
}

Future<void> showCollabHostDialog(BuildContext context, WidgetRef ref) async {
  final msg = await showDialog<String>(
    context: context,
    builder: (_) => const _CollabDialog(mode: _Collab.host),
  );
  if (msg == null) return;
  refreshNet(ref);
  if (context.mounted) _ok(context, msg);
}

Future<void> showCollabJoinDialog(BuildContext context, WidgetRef ref) async {
  final msg = await showDialog<String>(
    context: context,
    builder: (_) => const _CollabDialog(mode: _Collab.join),
  );
  if (msg == null) return;
  refreshNet(ref);
  if (context.mounted) _ok(context, msg);
}

class _McpServerDialog extends ConsumerStatefulWidget {
  const _McpServerDialog();
  @override
  ConsumerState<_McpServerDialog> createState() => _McpServerDialogState();
}

class _McpServerDialogState extends ConsumerState<_McpServerDialog> {
  final _name = TextEditingController();
  final _url = TextEditingController();
  final _command = TextEditingController();
  final _token = TextEditingController();
  String? _error;
  bool _busy = false;

  @override
  void dispose() {
    _name.dispose();
    _url.dispose();
    _command.dispose();
    _token.dispose();
    super.dispose();
  }

  Future<void> _go() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final msg = await ref
          .read(apiProvider)
          .mcpAddServer(
            _name.text.trim(),
            url: _url.text.trim(),
            command: _command.text.trim(),
            token: _token.text,
          );
      _token.clear();
      if (mounted) Navigator.pop(context, msg);
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Add MCP server'),
    content: SizedBox(
      width: 420,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          TextField(
            key: const ValueKey('mcp-server-name'),
            controller: _name,
            decoration: const InputDecoration(labelText: 'Name'),
          ),
          TextField(
            key: const ValueKey('mcp-server-url'),
            controller: _url,
            decoration: const InputDecoration(labelText: 'URL (http or https)'),
          ),
          TextField(
            key: const ValueKey('mcp-server-command'),
            controller: _command,
            decoration: const InputDecoration(labelText: 'Or a command'),
          ),
          TextField(
            key: const ValueKey('mcp-server-token'),
            controller: _token,
            obscureText: true,
            decoration: const InputDecoration(
              labelText: 'Bearer token (optional)',
            ),
          ),
          if (_error != null)
            Text(_error!, style: TextStyle(color: context.muninn.error)),
        ],
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        key: const ValueKey('mcp-add-server'),
        onPressed: _busy ? null : _go,
        child: const Text('Add'),
      ),
    ],
  );
}

class _McpClientDialog extends ConsumerStatefulWidget {
  const _McpClientDialog();
  @override
  ConsumerState<_McpClientDialog> createState() => _McpClientDialogState();
}

class _McpClientDialogState extends ConsumerState<_McpClientDialog> {
  final _name = TextEditingController();
  String _role = 'read';
  String? _error;
  bool _busy = false;

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  Future<void> _go() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final added = await ref
          .read(apiProvider)
          .mcpAddClient(_name.text.trim(), role: _role);
      if (mounted) Navigator.pop(context, added);
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Add MCP client'),
    content: SizedBox(
      width: 420,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          TextField(
            key: const ValueKey('mcp-client-name'),
            controller: _name,
            decoration: const InputDecoration(labelText: 'Name'),
          ),
          DropdownButtonFormField<String>(
            key: const ValueKey('mcp-client-role'),
            initialValue: _role,
            items: const [
              DropdownMenuItem(value: 'read', child: Text('read')),
              DropdownMenuItem(value: 'write', child: Text('write')),
              DropdownMenuItem(value: 'run', child: Text('run')),
              DropdownMenuItem(value: 'full', child: Text('full')),
            ],
            onChanged: (v) => setState(() => _role = v ?? 'read'),
            decoration: const InputDecoration(labelText: 'Role'),
          ),
          if (_error != null)
            Text(_error!, style: TextStyle(color: context.muninn.error)),
        ],
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        key: const ValueKey('mcp-add-client'),
        onPressed: _busy ? null : _go,
        child: const Text('Add'),
      ),
    ],
  );
}

class _PluginInstallDialog extends ConsumerStatefulWidget {
  const _PluginInstallDialog();
  @override
  ConsumerState<_PluginInstallDialog> createState() =>
      _PluginInstallDialogState();
}

class _PluginInstallDialogState extends ConsumerState<_PluginInstallDialog> {
  final _what = TextEditingController();
  final _name = TextEditingController();
  String _kind = 'parser';
  String? _error;
  bool _busy = false;

  @override
  void dispose() {
    _what.dispose();
    _name.dispose();
    super.dispose();
  }

  Future<void> _go() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final msg = await ref
          .read(apiProvider)
          .pluginInstall(
            _what.text.trim(),
            kind: _kind,
            name: _name.text.trim(),
          );
      if (mounted) Navigator.pop(context, msg);
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Install plugin'),
    content: SizedBox(
      width: 420,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          TextField(
            key: const ValueKey('plugin-what'),
            controller: _what,
            decoration: const InputDecoration(
              labelText: 'Library path or code name',
            ),
          ),
          Align(
            alignment: Alignment.centerLeft,
            child: TextButton(
              onPressed: () async {
                final picked = await FilePicker.platform.pickFiles();
                final path = picked?.files.single.path;
                if (path != null) _what.text = path;
              },
              child: const Text('Choose a library…'),
            ),
          ),
          DropdownButtonFormField<String>(
            key: const ValueKey('plugin-kind'),
            initialValue: _kind,
            items: const [
              DropdownMenuItem(value: 'parser', child: Text('parser')),
              DropdownMenuItem(value: 'transform', child: Text('transform')),
              DropdownMenuItem(value: 'analysis', child: Text('analysis')),
              DropdownMenuItem(
                value: 'ingest-connector',
                child: Text('ingest connector'),
              ),
            ],
            onChanged: (v) => setState(() => _kind = v ?? 'parser'),
            decoration: const InputDecoration(labelText: 'Kind'),
          ),
          TextField(
            key: const ValueKey('plugin-name'),
            controller: _name,
            decoration: const InputDecoration(labelText: 'Name (optional)'),
          ),
          if (_error != null)
            Text(_error!, style: TextStyle(color: context.muninn.error)),
        ],
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        key: const ValueKey('plugin-install'),
        onPressed: _busy ? null : _go,
        child: const Text('Install'),
      ),
    ],
  );
}

class _AnnounceDialog extends ConsumerStatefulWidget {
  const _AnnounceDialog();
  @override
  ConsumerState<_AnnounceDialog> createState() => _AnnounceDialogState();
}

class _AnnounceDialogState extends ConsumerState<_AnnounceDialog> {
  final _org = TextEditingController();
  bool _closed = false;
  String? _error;
  bool _busy = false;

  @override
  void dispose() {
    _org.dispose();
    super.dispose();
  }

  Future<void> _go() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final lines = await ref
          .read(apiProvider)
          .orgAnnounce(org: _org.text.trim(), closed: _closed);
      if (mounted) Navigator.pop(context, lines);
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Announce site'),
    content: SizedBox(
      width: 420,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          TextField(
            key: const ValueKey('org-announce-key'),
            controller: _org,
            decoration: const InputDecoration(
              labelText: 'Org (empty announces every hosted org)',
            ),
          ),
          CheckboxListTile(
            key: const ValueKey('org-announce-closed'),
            value: _closed,
            onChanged: (v) => setState(() => _closed = v ?? false),
            title: const Text('Announce it closed'),
            contentPadding: EdgeInsets.zero,
          ),
          if (_error != null)
            Text(_error!, style: TextStyle(color: context.muninn.error)),
        ],
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        key: const ValueKey('org-announce'),
        onPressed: _busy ? null : _go,
        child: const Text('Announce'),
      ),
    ],
  );
}

class _OfferDialog extends ConsumerStatefulWidget {
  const _OfferDialog();
  @override
  ConsumerState<_OfferDialog> createState() => _OfferDialogState();
}

class _OfferDialogState extends ConsumerState<_OfferDialog> {
  final _label = TextEditingController();
  final _peer = TextEditingController();
  String? _error;
  bool _busy = false;

  @override
  void dispose() {
    _label.dispose();
    _peer.dispose();
    super.dispose();
  }

  Future<void> _go() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final once = await ref
          .read(apiProvider)
          .netExchangeOffer(label: _label.text.trim(), peer: _peer.text.trim());
      if (mounted) Navigator.pop(context, once);
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Offer a key exchange'),
    content: SizedBox(
      width: 420,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          TextField(
            key: const ValueKey('net-offer-label'),
            controller: _label,
            decoration: const InputDecoration(labelText: 'Label'),
          ),
          TextField(
            key: const ValueKey('net-offer-peer'),
            controller: _peer,
            decoration: const InputDecoration(
              labelText: 'Peer (optional; otherwise copy the token)',
            ),
          ),
          if (_error != null)
            Text(_error!, style: TextStyle(color: context.muninn.error)),
        ],
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        key: const ValueKey('net-exchange-offer'),
        onPressed: _busy ? null : _go,
        child: const Text('Create'),
      ),
    ],
  );
}

class _AcceptDialog extends ConsumerStatefulWidget {
  const _AcceptDialog();
  @override
  ConsumerState<_AcceptDialog> createState() => _AcceptDialogState();
}

class _AcceptDialogState extends ConsumerState<_AcceptDialog> {
  final _token = TextEditingController();
  final _label = TextEditingController();
  String? _error;
  bool _busy = false;

  @override
  void dispose() {
    _token.dispose();
    _label.dispose();
    super.dispose();
  }

  Future<void> _go() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final once = await ref
          .read(apiProvider)
          .netExchangeAccept(_token.text.trim(), label: _label.text.trim());
      _token.clear();
      if (mounted) Navigator.pop(context, once);
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Accept a key exchange'),
    content: SizedBox(
      width: 420,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          TextField(
            key: const ValueKey('net-accept-token'),
            controller: _token,
            decoration: const InputDecoration(
              labelText: 'Offer or answer token',
            ),
          ),
          TextField(
            controller: _label,
            decoration: const InputDecoration(labelText: 'Label (optional)'),
          ),
          if (_error != null)
            Text(_error!, style: TextStyle(color: context.muninn.error)),
        ],
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        key: const ValueKey('net-exchange-accept'),
        onPressed: _busy ? null : _go,
        child: const Text('Accept'),
      ),
    ],
  );
}

class _ShardRestoreDialog extends ConsumerStatefulWidget {
  const _ShardRestoreDialog();
  @override
  ConsumerState<_ShardRestoreDialog> createState() =>
      _ShardRestoreDialogState();
}

class _ShardRestoreDialogState extends ConsumerState<_ShardRestoreDialog> {
  final _file = TextEditingController();
  final _backupPw = TextEditingController();
  final _shardPw = TextEditingController();
  final _newPw = TextEditingController();
  final _peers = TextEditingController();
  String? _error;
  bool _busy = false;

  @override
  void dispose() {
    _file.dispose();
    _backupPw.dispose();
    _shardPw.dispose();
    _newPw.dispose();
    _peers.dispose();
    super.dispose();
  }

  Future<void> _go() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final peers = _peers.text
          .split(RegExp(r'[,\s]+'))
          .where((s) => s.isNotEmpty)
          .toList();
      final msg = await ref
          .read(apiProvider)
          .netShardRestore(
            backup: _file.text.trim(),
            backupPassword: _backupPw.text,
            shardPassword: _shardPw.text,
            newPassword: _newPw.text,
            peers: peers,
          );
      _backupPw.clear();
      _shardPw.clear();
      _newPw.clear();
      if (mounted) Navigator.pop(context, msg);
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Restore from a shard backup'),
    content: SizedBox(
      width: 460,
      child: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Text(
              'Use this when the database will not open. The new password replaces the old one. Words are not printed.',
            ),
            TextField(
              key: const ValueKey('shard-backup-file'),
              controller: _file,
              decoration: const InputDecoration(labelText: 'Backup file'),
            ),
            Align(
              alignment: Alignment.centerLeft,
              child: TextButton(
                onPressed: () async {
                  final picked = await FilePicker.platform.pickFiles();
                  final path = picked?.files.single.path;
                  if (path != null) _file.text = path;
                },
                child: const Text('Choose backup…'),
              ),
            ),
            TextField(
              key: const ValueKey('shard-backup-password'),
              controller: _backupPw,
              obscureText: true,
              decoration: const InputDecoration(labelText: 'Backup password'),
            ),
            TextField(
              key: const ValueKey('shard-password'),
              controller: _shardPw,
              obscureText: true,
              decoration: const InputDecoration(labelText: 'Shard password'),
            ),
            TextField(
              key: const ValueKey('shard-new-password'),
              controller: _newPw,
              obscureText: true,
              decoration: const InputDecoration(
                labelText: 'New Muninn password',
              ),
            ),
            TextField(
              key: const ValueKey('shard-peers'),
              controller: _peers,
              decoration: const InputDecoration(
                labelText: 'Peers (optional, comma separated)',
              ),
            ),
            if (_error != null)
              Text(_error!, style: TextStyle(color: context.muninn.error)),
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
        key: const ValueKey('shard-restore-go'),
        onPressed: _busy ? null : _go,
        child: const Text('Restore'),
      ),
    ],
  );
}

enum _Collab { send, host, join }

class _CollabDialog extends ConsumerStatefulWidget {
  const _CollabDialog({required this.mode});
  final _Collab mode;
  @override
  ConsumerState<_CollabDialog> createState() => _CollabDialogState();
}

class _CollabDialogState extends ConsumerState<_CollabDialog> {
  final _peer = TextEditingController();
  final _text = TextEditingController();
  final _kind = TextEditingController();
  final _key = TextEditingController();
  final _session = TextEditingController();
  String? _error;
  bool _busy = false;

  @override
  void dispose() {
    _peer.dispose();
    _text.dispose();
    _kind.dispose();
    _key.dispose();
    _session.dispose();
    super.dispose();
  }

  Future<void> _go() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final api = ref.read(apiProvider);
      final msg = switch (widget.mode) {
        _Collab.send => await api.collabSend(_peer.text.trim(), _text.text),
        _Collab.host => await api.collabHost(
          _peer.text.trim(),
          _kind.text.trim(),
          _key.text.trim(),
        ),
        _Collab.join => await api.collabJoin(
          _peer.text.trim(),
          _session.text.trim(),
        ),
      };
      if (mounted) Navigator.pop(context, msg);
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final title = switch (widget.mode) {
      _Collab.send => 'Send a message',
      _Collab.host => 'Host a live session',
      _Collab.join => 'Join a live session',
    };
    final goKey = switch (widget.mode) {
      _Collab.send => 'collab-send',
      _Collab.host => 'collab-host',
      _Collab.join => 'collab-join',
    };
    return AlertDialog(
      title: Text(title),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              key: const ValueKey('collab-peer'),
              controller: _peer,
              decoration: const InputDecoration(labelText: 'Peer'),
            ),
            if (widget.mode == _Collab.send)
              TextField(
                key: const ValueKey('collab-text'),
                controller: _text,
                decoration: const InputDecoration(labelText: 'Message'),
              ),
            if (widget.mode == _Collab.host) ...[
              TextField(
                key: const ValueKey('collab-kind'),
                controller: _kind,
                decoration: const InputDecoration(
                  labelText: 'Kind (journal, project, code…)',
                ),
              ),
              TextField(
                key: const ValueKey('collab-key'),
                controller: _key,
                decoration: const InputDecoration(labelText: 'Name'),
              ),
            ],
            if (widget.mode == _Collab.join)
              TextField(
                key: const ValueKey('collab-session'),
                controller: _session,
                decoration: const InputDecoration(labelText: 'Session'),
              ),
            if (_error != null)
              Text(_error!, style: TextStyle(color: context.muninn.error)),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: ValueKey(goKey),
          onPressed: _busy ? null : _go,
          child: const Text('Go'),
        ),
      ],
    );
  }
}
