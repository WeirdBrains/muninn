/// TUI Net ▸ MCP… (`mnnn mcp`). Tokens and credential values stay in Rust.
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

class McpPane extends ConsumerWidget {
  const McpPane({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final servers = ref.watch(mcpServersProvider);
    final clients = ref.watch(mcpClientsProvider);
    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
      children: [
        Text(
          'External MCP servers (their tools and approvals) and the clients allowed to call this node. Tokens and credential values stay in Rust. Adding a server or client is `mnnn mcp add` / `mnnn mcp client add`.',
          style: TextStyle(color: m.muted),
        ),
        const SizedBox(height: 16),
        Text('Servers', style: TextStyle(color: m.header, fontSize: 16)),
        servers.when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Text('$e', style: TextStyle(color: m.error)),
          data: (items) {
            final all = items ?? const <McpServerInfo>[];
            if (all.isEmpty) {
              return Text(
                'No MCP servers (`mnnn mcp add NAME --url URL` or `-- COMMAND`).',
                style: TextStyle(color: m.muted),
              );
            }
            return Column(
              children: [
                for (final s in all)
                  _ServerTile(key: ValueKey('mcp-server-${s.id}'), server: s),
              ],
            );
          },
        ),
        const SizedBox(height: 16),
        Text('Clients', style: TextStyle(color: m.header, fontSize: 16)),
        clients.when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Text('$e', style: TextStyle(color: m.error)),
          data: (items) {
            final all = items ?? const <McpClientInfo>[];
            if (all.isEmpty) {
              return Text(
                'No MCP clients (`mnnn mcp client add NAME`).',
                style: TextStyle(color: m.muted),
              );
            }
            return Column(
              children: [
                for (final c in all)
                  _ClientTile(key: ValueKey('mcp-client-${c.id}'), client: c),
              ],
            );
          },
        ),
      ],
    );
  }
}

class _ServerTile extends ConsumerWidget {
  const _ServerTile({super.key, required this.server});
  final McpServerInfo server;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return ExpansionTile(
      tilePadding: EdgeInsets.zero,
      title: Text(server.name),
      subtitle: Text(
        [
          server.transport,
          if (server.url.isNotEmpty) server.url,
          if (server.command.isNotEmpty)
            ([server.command, ...server.args].join(' ')),
          if (server.credentials.isNotEmpty) server.credentials.join(', '),
        ].join(' · '),
        style: TextStyle(color: m.muted),
      ),
      trailing: PopupMenuButton<String>(
        onSelected: (v) => _act(context, ref, v),
        itemBuilder: (_) => const [
          PopupMenuItem(value: 'tools', child: Text('Tools…')),
          PopupMenuItem(value: 'remove', child: Text('Remove')),
        ],
      ),
      children: [
        Align(
          alignment: Alignment.centerLeft,
          child: TextButton(
            key: ValueKey('mcp-tools-${server.id}'),
            onPressed: () => _act(context, ref, 'tools'),
            child: const Text('Show tools…'),
          ),
        ),
      ],
    );
  }

  Future<void> _act(BuildContext context, WidgetRef ref, String v) async {
    try {
      switch (v) {
        case 'tools':
          final tools = await ref.read(apiProvider).mcpServerTools(server.id);
          if (!context.mounted) return;
          await showDialog<void>(
            context: context,
            builder: (ctx) => _ToolsDialog(server: server, tools: tools),
          );
          refreshNet(ref);
        case 'remove':
          final msg = await ref.read(apiProvider).mcpRemoveServer(server.id);
          refreshNet(ref);
          if (context.mounted) _ok(context, msg);
      }
    } catch (e) {
      if (context.mounted) _ok(context, '$e', color: mError(context));
    }
  }
}

Color mError(BuildContext context) => context.muninn.error;

class _ToolsDialog extends ConsumerStatefulWidget {
  const _ToolsDialog({required this.server, required this.tools});
  final McpServerInfo server;
  final List<McpToolInfo> tools;
  @override
  ConsumerState<_ToolsDialog> createState() => _ToolsDialogState();
}

class _ToolsDialogState extends ConsumerState<_ToolsDialog> {
  late List<McpToolInfo> _tools;

  @override
  void initState() {
    super.initState();
    _tools = List.of(widget.tools);
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      backgroundColor: m.bg,
      title: Text(
        'Tools · ${widget.server.name}',
        style: TextStyle(color: m.header),
      ),
      content: SizedBox(
        width: 480,
        child: _tools.isEmpty
            ? Text('No tools.', style: TextStyle(color: m.muted))
            : SingleChildScrollView(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    for (final tool in _tools)
                      SwitchListTile(
                        key: ValueKey('mcp-tool-${tool.name}'),
                        title: Text(tool.name),
                        subtitle: Text(
                          tool.description,
                          style: TextStyle(color: m.muted),
                        ),
                        value: tool.approved,
                        onChanged: (on) async {
                          try {
                            final msg = await ref
                                .read(apiProvider)
                                .mcpApproveTool(
                                  widget.server.id,
                                  tool.name,
                                  on: on,
                                );
                            setState(() {
                              _tools = [
                                for (final x in _tools)
                                  if (x.name == tool.name)
                                    McpToolInfo(
                                      name: x.name,
                                      description: x.description,
                                      approved: on,
                                    )
                                  else
                                    x,
                              ];
                            });
                            if (context.mounted) _ok(context, msg);
                          } catch (e) {
                            if (context.mounted) {
                              _ok(context, '$e', color: m.error);
                            }
                          }
                        },
                      ),
                  ],
                ),
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

class _ClientTile extends ConsumerWidget {
  const _ClientTile({super.key, required this.client});
  final McpClientInfo client;

  static const _roles = ['read', 'write', 'run', 'full'];

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return ListTile(
      contentPadding: EdgeInsets.zero,
      title: Text(client.name),
      subtitle: Text(
        [client.role, if (client.revoked) 'revoked'].join(' · '),
        style: TextStyle(color: client.revoked ? m.warning : m.muted),
      ),
      trailing: PopupMenuButton<String>(
        onSelected: (v) => _act(context, ref, v),
        itemBuilder: (_) => [
          for (final r in _roles)
            PopupMenuItem(value: 'role:$r', child: Text('Role: $r')),
          PopupMenuItem(
            value: client.revoked ? 'restore' : 'revoke',
            child: Text(client.revoked ? 'Restore' : 'Revoke'),
          ),
          const PopupMenuItem(value: 'remove', child: Text('Remove')),
        ],
      ),
    );
  }

  Future<void> _act(BuildContext context, WidgetRef ref, String v) async {
    try {
      final api = ref.read(apiProvider);
      late final String msg;
      if (v.startsWith('role:')) {
        msg = await api.mcpClientRole(client.id, v.substring(5));
      } else if (v == 'revoke' || v == 'restore') {
        msg = await api.mcpClientRevoke(client.id, revoke: v == 'revoke');
      } else if (v == 'remove') {
        msg = await api.mcpRemoveClient(client.id);
      } else {
        return;
      }
      refreshNet(ref);
      if (context.mounted) _ok(context, msg);
    } catch (e) {
      if (context.mounted) _ok(context, '$e', color: context.muninn.error);
    }
  }
}
