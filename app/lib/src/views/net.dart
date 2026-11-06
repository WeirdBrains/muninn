/// Net: `mnnn net status|peers` plus start/stop, connect, invites, relays,
/// sharing, settings, connections, exchanges, search, broadcast and dark shards.
/// The node identity, exchange tokens and shard blobs stay in Rust; invite links
/// hold no secret. Short codes are meant to be read aloud.
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../state.dart';
import '../theme/muninn_theme.dart';
import '../time_range.dart';
import 'net_dialogs.dart';
import 'unlock.dart';

void _ok(BuildContext context, String msg, {Color? color}) {
  ScaffoldMessenger.of(context)
      .showSnackBar(SnackBar(content: Text(msg), backgroundColor: color));
}

class NetView extends ConsumerWidget {
  const NetView({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final pane = ref.watch(netPaneProvider);
    return Unlocked(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(12, 8, 12, 0),
            child: Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                for (final p in NetPane.values)
                  FilterChip(
                    key: ValueKey('net-pane-${p.name}'),
                    label: Text(_paneTitle(p)),
                    selected: pane == p,
                    onSelected: (_) =>
                        ref.read(netPaneProvider.notifier).state = p,
                  ),
              ],
            ),
          ),
          Expanded(
            child: switch (pane) {
              NetPane.node => _NodePane(),
              NetPane.peers => _PeersPane(),
              NetPane.relays => _RelaysPane(),
              NetPane.invites => _InvitesPane(),
              NetPane.shared => _SharedPane(),
              NetPane.settings => _SettingsPane(),
              NetPane.connections => _ConnectionsPane(),
              NetPane.exchanges => _ExchangesPane(),
              NetPane.search => _SearchPane(),
              NetPane.shards => _ShardsPane(),
            },
          ),
        ],
      ),
    );
  }

  static String _paneTitle(NetPane p) => switch (p) {
    NetPane.node => 'Node',
    NetPane.peers => 'Peers',
    NetPane.relays => 'Relays',
    NetPane.invites => 'Invites',
    NetPane.shared => 'Shared',
    NetPane.settings => 'Settings',
    NetPane.connections => 'Connections',
    NetPane.exchanges => 'Nodes',
    NetPane.search => 'Search',
    NetPane.shards => 'Shards',
  };
}

class _NodePane extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return ref
        .watch(netOverviewProvider)
        .when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Padding(
            padding: const EdgeInsets.all(16),
            child: Text('$e', style: TextStyle(color: m.error)),
          ),
          data: (o) {
            if (o == null) {
              return const SizedBox.shrink();
            }
            return ListView(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
              children: [
                _NodeStatus(o.status),
                const SizedBox(height: 16),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    if (!o.status.running)
                      FilledButton.icon(
                        key: const ValueKey('net-start'),
                        onPressed: () => netStartNode(context, ref),
                        icon: const Icon(Icons.play_arrow),
                        label: const Text('Start node'),
                      )
                    else
                      OutlinedButton.icon(
                        key: const ValueKey('net-stop'),
                        onPressed: () => netStopNode(context, ref),
                        icon: const Icon(Icons.stop),
                        label: const Text('Stop node'),
                      ),
                    OutlinedButton(
                      onPressed: () =>
                          showNetInviteOrConnectDialog(context, ref),
                      child: const Text('Invite or connect…'),
                    ),
                  ],
                ),
              ],
            );
          },
        );
  }
}

class _NodeStatus extends StatelessWidget {
  const _NodeStatus(this.s);
  final NetStatus s;

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    final running = s.running
        ? 'yes${s.pid == null ? '' : ' (pid ${s.pid}${s.startedAt == null ? '' : ', since ${fmtUtc(s.startedAt!)} UTC'})'}'
        : 'no';
    final exchanges =
        '${s.exchangesActive} active${s.incoming > 0 ? ', ${s.incoming} incoming offer(s)' : ''}';
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          'Node',
          style: TextStyle(
            color: m.header,
            fontWeight: FontWeight.w800,
            fontSize: 16,
          ),
        ),
        const SizedBox(height: 8),
        _row(m, 'Node', s.label),
        _row(
          m,
          'Peer id',
          s.peerId ?? '(made when the node first starts)',
          key: const ValueKey('net-peer-id'),
        ),
        _row(m, 'Running', running, key: const ValueKey('net-running')),
        if (s.fingerprint != null) _row(m, 'Identity', s.fingerprint!),
        if (s.running) ...[
          _row(m, 'NAT', s.nat.isEmpty ? 'unknown' : s.nat),
          _row(m, 'mDNS', s.mdns ? 'on' : 'off'),
          _row(m, 'Relay', s.relayServer ? 'serving' : 'not serving'),
          _row(m, 'Connected', '${s.connected}'),
          Text(
            'Reachable at',
            style: TextStyle(color: m.muted, fontWeight: FontWeight.w600),
          ),
          if (s.addresses.isEmpty)
            Padding(
              padding: const EdgeInsets.only(left: 12, top: 4, bottom: 4),
              child: Text(
                '(no address others can dial yet)',
                style: TextStyle(color: m.muted),
              ),
            )
          else
            for (final a in s.addresses)
              Padding(
                padding: const EdgeInsets.only(left: 12, top: 4),
                child: SelectableText(
                  a,
                  style: TextStyle(
                    color: m.fg,
                    fontFamily: 'monospace',
                    fontSize: 12,
                  ),
                ),
              ),
          if (s.reservations.isNotEmpty) ...[
            Text(
              'Reserved',
              style: TextStyle(color: m.muted, fontWeight: FontWeight.w600),
            ),
            for (final r in s.reservations)
              Padding(
                padding: const EdgeInsets.only(left: 12, top: 4),
                child: SelectableText(
                  r,
                  style: TextStyle(
                    color: m.fg,
                    fontFamily: 'monospace',
                    fontSize: 12,
                  ),
                ),
              ),
          ],
        ],
        _row(m, 'Exchanges', exchanges),
        _row(m, 'Shared', '${s.shared} item(s)'),
      ],
    );
  }

  Widget _row(MuninnColors m, String k, String v, {Key? key}) => Padding(
    key: key,
    padding: const EdgeInsets.only(bottom: 6),
    child: Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SizedBox(
          width: 110,
          child: Text(k, style: TextStyle(color: m.muted)),
        ),
        Expanded(
          child: SelectableText(v, style: TextStyle(color: m.fg)),
        ),
      ],
    ),
  );
}

class _PeersPane extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return ref
        .watch(netOverviewProvider)
        .when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Padding(
            padding: const EdgeInsets.all(16),
            child: Text('$e', style: TextStyle(color: m.error)),
          ),
          data: (o) {
            final peers = o?.peers ?? const <NetPeerInfo>[];
            return ListView(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
              children: [
                Align(
                  alignment: Alignment.centerLeft,
                  child: FilledButton.icon(
                    key: const ValueKey('net-connect'),
                    onPressed: () => showNetConnectDialog(context, ref),
                    icon: const Icon(Icons.link),
                    label: const Text('Connect…'),
                  ),
                ),
                const SizedBox(height: 12),
                if (peers.isEmpty)
                  Text(
                    'No peers yet: start the node or connect to an address or invite link.',
                    style: TextStyle(color: m.muted),
                  )
                else
                  for (final p in peers) _PeerTile(p),
              ],
            );
          },
        );
  }
}

class _PeerTile extends ConsumerWidget {
  const _PeerTile(this.p);
  final NetPeerInfo p;

  Color _linkColor(MuninnColors m) => switch (p.link) {
    'connected' => m.success,
    'requested' || 'pending' => m.warning,
    'failed' => m.error,
    _ => m.muted,
  };

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final name = p.label.isEmpty ? p.shortId : p.label;
    final seen = p.lastSeen == 0 ? '' : ' · seen ${fmtUtc(p.lastSeen)} UTC';
    final who = p.user.isEmpty ? '' : ' · ${p.user}';
    return ListTile(
      contentPadding: EdgeInsets.zero,
      leading: Icon(Icons.circle, size: 12, color: _linkColor(m)),
      title: Text(name),
      subtitle: Text(
        '${p.shortId}$who · ${p.link} · ${p.source} · ${p.caps}$seen',
        style: TextStyle(color: m.muted),
      ),
      trailing: PopupMenuButton<String>(
        onSelected: (v) async {
          final key = p.label.isEmpty ? p.peerId : p.label;
          if (v == 'connect') {
            await showNetConnectDialog(context, ref, initial: p.peerId);
          } else if (v == 'disconnect') {
            if (!await confirmNet(
              context,
              'Disconnect',
              'End the connection with $name? Keys are erased on both sides.',
            )) {
              return;
            }
            try {
              final msg = await ref.read(apiProvider).netDisconnect(key);
              refreshNet(ref);
              if (context.mounted) {
                _ok(context, msg);
              }
            } catch (e) {
              if (context.mounted) {
                _ok(context, '$e', color: m.error);
              }
            }
          } else if (v == 'forget') {
            if (!await confirmNet(
              context,
              'Forget peer',
              'Forget $name? Exchanges with it are kept.',
            )) {
              return;
            }
            try {
              final msg = await ref.read(apiProvider).netForget(key);
              refreshNet(ref);
              if (context.mounted) {
                _ok(context, msg);
              }
            } catch (e) {
              if (context.mounted) {
                _ok(context, '$e', color: m.error);
              }
            }
          }
        },
        itemBuilder: (_) => const [
          PopupMenuItem(value: 'connect', child: Text('Connect…')),
          PopupMenuItem(value: 'disconnect', child: Text('Disconnect')),
          PopupMenuItem(value: 'forget', child: Text('Forget')),
        ],
      ),
    );
  }
}

class _RelaysPane extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return ref
        .watch(netRelaysProvider)
        .when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Padding(
            padding: const EdgeInsets.all(16),
            child: Text('$e', style: TextStyle(color: m.error)),
          ),
          data: (r) {
            if (r == null) {
              return const SizedBox.shrink();
            }
            return ListView(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
              children: [
                Text(
                  'Relays',
                  style: TextStyle(
                    color: m.header,
                    fontWeight: FontWeight.w800,
                    fontSize: 16,
                  ),
                ),
                SwitchListTile(
                  key: const ValueKey('net-relay-serve'),
                  contentPadding: EdgeInsets.zero,
                  title: const Text('Relay for others'),
                  subtitle: Text(
                    r.serving
                        ? 'this node will relay (from the next start)'
                        : 'this node will not relay',
                    style: TextStyle(color: m.muted),
                  ),
                  value: r.serving,
                  onChanged: (on) async {
                    try {
                      final msg = await ref.read(apiProvider).netRelayServe(on);
                      refreshNet(ref);
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
                Align(
                  alignment: Alignment.centerLeft,
                  child: FilledButton.icon(
                    key: const ValueKey('net-relay-add'),
                    onPressed: () => showNetRelayAddDialog(context, ref),
                    icon: const Icon(Icons.add),
                    label: const Text('Add relay'),
                  ),
                ),
                const SizedBox(height: 12),
                if (r.configured.isEmpty)
                  Text(
                    'No relays configured.',
                    style: TextStyle(color: m.muted),
                  )
                else
                  for (final addr in r.configured)
                    ListTile(
                      key: ValueKey('net-relay-$addr'),
                      contentPadding: EdgeInsets.zero,
                      title: SelectableText(
                        addr,
                        style: const TextStyle(
                          fontFamily: 'monospace',
                          fontSize: 13,
                        ),
                      ),
                      trailing: IconButton(
                        tooltip: 'Remove',
                        onPressed: () async {
                          try {
                            final msg = await ref
                                .read(apiProvider)
                                .netRelayRemove(addr);
                            refreshNet(ref);
                            if (context.mounted) {
                              _ok(context, msg);
                            }
                          } catch (e) {
                            if (context.mounted) {
                              _ok(context, '$e', color: m.error);
                            }
                          }
                        },
                        icon: const Icon(Icons.delete_outline),
                      ),
                    ),
                if (r.seen.isNotEmpty) ...[
                  const SizedBox(height: 16),
                  Text(
                    'Seen announcing relay',
                    style: TextStyle(
                      color: m.header,
                      fontWeight: FontWeight.w700,
                    ),
                  ),
                  for (final p in r.seen)
                    ListTile(
                      contentPadding: EdgeInsets.zero,
                      title: Text(p.label.isEmpty ? p.shortId : p.label),
                      subtitle: Text(
                        '${p.shortId} · ${p.source}',
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

class _InvitesPane extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return ref
        .watch(netInvitesProvider)
        .when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Padding(
            padding: const EdgeInsets.all(16),
            child: Text('$e', style: TextStyle(color: m.error)),
          ),
          data: (inv) {
            final ours = inv?.invites ?? const <NetInviteInfo>[];
            final reqs = inv?.requests ?? const <NetRequestInfo>[];
            return ListView(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
              children: [
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    FilledButton.icon(
                      key: const ValueKey('net-invite'),
                      onPressed: () => showNetInviteDialog(context, ref),
                      icon: const Icon(Icons.mail_outline),
                      label: const Text('Create invite'),
                    ),
                    OutlinedButton(
                      onPressed: () =>
                          showNetInviteOrConnectDialog(context, ref),
                      child: const Text('Invite or connect…'),
                    ),
                  ],
                ),
                const SizedBox(height: 16),
                Text(
                  'Your invites',
                  style: TextStyle(
                    color: m.header,
                    fontWeight: FontWeight.w800,
                    fontSize: 16,
                  ),
                ),
                if (ours.isEmpty)
                  Padding(
                    padding: const EdgeInsets.symmetric(vertical: 8),
                    child: Text(
                      'None yet. Create a link; it holds no secret.',
                      style: TextStyle(color: m.muted),
                    ),
                  )
                else
                  for (final x in ours)
                    ListTile(
                      contentPadding: EdgeInsets.zero,
                      title: Text(x.title.isEmpty ? x.shortId : x.title),
                      subtitle: SelectableText(
                        x.link,
                        style: TextStyle(
                          color: m.muted,
                          fontFamily: 'monospace',
                          fontSize: 12,
                        ),
                      ),
                      trailing: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          IconButton(
                            tooltip: 'Copy link',
                            onPressed: () =>
                                Clipboard.setData(ClipboardData(text: x.link)),
                            icon: const Icon(Icons.copy),
                          ),
                          IconButton(
                            tooltip: 'Withdraw',
                            onPressed: () async {
                              if (!await confirmNet(
                                context,
                                'Withdraw invite',
                                'Withdraw the invite for ${x.title.isEmpty ? x.shortId : x.title}?',
                              )) {
                                return;
                              }
                              try {
                                final msg = await ref
                                    .read(apiProvider)
                                    .netDisconnect(x.id);
                                refreshNet(ref);
                                if (context.mounted) {
                                  _ok(context, msg);
                                }
                              } catch (e) {
                                if (context.mounted) {
                                  _ok(context, '$e', color: m.error);
                                }
                              }
                            },
                            icon: const Icon(Icons.delete_outline),
                          ),
                        ],
                      ),
                    ),
                const SizedBox(height: 16),
                Text(
                  'Requests waiting',
                  style: TextStyle(
                    color: m.header,
                    fontWeight: FontWeight.w800,
                    fontSize: 16,
                  ),
                ),
                if (reqs.isEmpty)
                  Padding(
                    padding: const EdgeInsets.symmetric(vertical: 8),
                    child: Text(
                      'No requests waiting.',
                      style: TextStyle(color: m.muted),
                    ),
                  )
                else
                  for (final x in reqs)
                    ListTile(
                      contentPadding: EdgeInsets.zero,
                      title: Text(x.title.isEmpty ? x.shortId : x.title),
                      subtitle: Text(
                        '${x.purpose} · ${x.shortId}',
                        style: TextStyle(color: m.muted),
                      ),
                      trailing: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          TextButton(
                            onPressed: () async {
                              try {
                                final r = await ref
                                    .read(apiProvider)
                                    .netAccept(x.id);
                                refreshNet(ref);
                                if (context.mounted) {
                                  _ok(context, '${r.status}: ${r.detail}');
                                }
                              } catch (e) {
                                if (context.mounted) {
                                  _ok(context, '$e', color: m.error);
                                }
                              }
                            },
                            child: const Text('Accept'),
                          ),
                          TextButton(
                            onPressed: () async {
                              try {
                                final msg = await ref
                                    .read(apiProvider)
                                    .netDecline(x.id);
                                refreshNet(ref);
                                if (context.mounted) {
                                  _ok(context, msg);
                                }
                              } catch (e) {
                                if (context.mounted) {
                                  _ok(context, '$e', color: m.error);
                                }
                              }
                            },
                            child: const Text('Decline'),
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

class _SharedPane extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return ref
        .watch(netSharedProvider)
        .when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Padding(
            padding: const EdgeInsets.all(16),
            child: Text('$e', style: TextStyle(color: m.error)),
          ),
          data: (items) {
            final all = items ?? const <NetShareInfo>[];
            return ListView(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
              children: [
                Align(
                  alignment: Alignment.centerLeft,
                  child: FilledButton.icon(
                    key: const ValueKey('net-share'),
                    onPressed: () => showNetShareDialog(context, ref),
                    icon: const Icon(Icons.ios_share),
                    label: const Text('Share…'),
                  ),
                ),
                const SizedBox(height: 12),
                if (all.isEmpty)
                  Text(
                    'Nothing is shared. Share a project, journal, search, model, report, code space or pipeline.',
                    style: TextStyle(color: m.muted),
                  )
                else
                  for (final s in all)
                    ListTile(
                      contentPadding: EdgeInsets.zero,
                      title: Text('${s.kind} ${s.name}'),
                      subtitle: Text(
                        'with ${s.withWhom}',
                        style: TextStyle(color: m.muted),
                      ),
                      trailing: IconButton(
                        tooltip: 'Unshare',
                        onPressed: () async {
                          if (!await confirmNet(
                            context,
                            'Stop sharing',
                            'Stop sharing ${s.kind} ${s.name}?',
                          )) {
                            return;
                          }
                          try {
                            final msg = await ref
                                .read(apiProvider)
                                .netUnshare(s.kind, s.name);
                            refreshNet(ref);
                            if (context.mounted) {
                              _ok(context, msg);
                            }
                          } catch (e) {
                            if (context.mounted) {
                              _ok(context, '$e', color: m.error);
                            }
                          }
                        },
                        icon: const Icon(Icons.link_off),
                      ),
                    ),
              ],
            );
          },
        );
  }
}

class _SettingsPane extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return ref
        .watch(netConfigProvider)
        .when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Padding(
            padding: const EdgeInsets.all(16),
            child: Text('$e', style: TextStyle(color: m.error)),
          ),
          data: (cfg) {
            if (cfg == null) {
              return const SizedBox.shrink();
            }
            return _SettingsForm(config: cfg);
          },
        );
  }
}

class _SettingsForm extends ConsumerStatefulWidget {
  const _SettingsForm({required this.config});
  final NetConfig config;
  @override
  ConsumerState<_SettingsForm> createState() => _SettingsFormState();
}

class _SettingsFormState extends ConsumerState<_SettingsForm> {
  late final TextEditingController _label;
  late final TextEditingController _port;
  late final TextEditingController _listen;
  late final TextEditingController _bootstrap;
  late final TextEditingController _external;
  late final TextEditingController _collabBind;
  late final TextEditingController _collabPort;
  late final TextEditingController _collabAdvertise;
  late bool _mdns;
  late bool _relayServer;
  late bool _autostart;
  late bool _hostAutostart;
  late String _accept;
  late String _collabTransport;

  @override
  void initState() {
    super.initState();
    final c = widget.config;
    _label = TextEditingController(text: c.label);
    _port = TextEditingController(text: '${c.port}');
    _listen = TextEditingController(text: c.listen.join('\n'));
    _bootstrap = TextEditingController(text: c.bootstrap.join('\n'));
    _external = TextEditingController(text: c.externalAddresses.join('\n'));
    _collabBind = TextEditingController(text: c.collabBind);
    _collabPort = TextEditingController(text: '${c.collabPort}');
    _collabAdvertise = TextEditingController(
      text: c.collabAdvertise.join('\n'),
    );
    _mdns = c.mdns;
    _relayServer = c.relayServer;
    _autostart = c.autostart;
    _hostAutostart = c.hostAutostart;
    _accept = c.accept;
    _collabTransport = c.collabTransport;
  }

  @override
  void dispose() {
    _label.dispose();
    _port.dispose();
    _listen.dispose();
    _bootstrap.dispose();
    _external.dispose();
    _collabBind.dispose();
    _collabPort.dispose();
    _collabAdvertise.dispose();
    super.dispose();
  }

  List<String> _lines(String s) => [
    for (final part in s.split(RegExp(r'[\n,]')))
      if (part.trim().isNotEmpty) part.trim(),
  ];

  Future<void> _save() async {
    final m = context.muninn;
    try {
      final msg = await ref
          .read(apiProvider)
          .netConfigSet(
            NetConfig(
              label: _label.text.trim(),
              port: int.tryParse(_port.text.trim()) ?? 4741,
              listen: _lines(_listen.text),
              mdns: _mdns,
              relayServer: _relayServer,
              autostart: _autostart,
              hostAutostart: _hostAutostart,
              accept: _accept,
              bootstrap: _lines(_bootstrap.text),
              externalAddresses: _lines(_external.text),
              collabTransport: _collabTransport,
              collabBind: _collabBind.text.trim(),
              collabPort: int.tryParse(_collabPort.text.trim()) ?? 4742,
              collabAdvertise: _lines(_collabAdvertise.text),
            ),
          );
      refreshNet(ref);
      if (mounted) {
        _ok(context, msg);
      }
    } catch (e) {
      if (mounted) {
        _ok(context, '$e', color: m.error);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Expanded(
          child: SingleChildScrollView(
            padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text(
                  'Network settings (preferences.toml, shared with the TUI). Relays stay on the Relays page. Changes apply the next time the node starts.',
                  style: TextStyle(color: m.muted),
                ),
                TextField(
                  key: const ValueKey('net-config-label'),
                  controller: _label,
                  decoration: const InputDecoration(labelText: 'Label'),
                ),
                TextField(
                  key: const ValueKey('net-config-port'),
                  controller: _port,
                  keyboardType: TextInputType.number,
                  decoration: const InputDecoration(labelText: 'Port'),
                ),
                TextField(
                  controller: _listen,
                  maxLines: 3,
                  decoration: const InputDecoration(
                    labelText: 'Listen addresses',
                    hintText: 'one per line, empty for the default',
                  ),
                ),
                SwitchListTile(
                  contentPadding: EdgeInsets.zero,
                  title: const Text('Find peers on the local network (mDNS)'),
                  value: _mdns,
                  onChanged: (v) => setState(() => _mdns = v),
                ),
                SwitchListTile(
                  contentPadding: EdgeInsets.zero,
                  title: const Text('Relay for others'),
                  value: _relayServer,
                  onChanged: (v) => setState(() => _relayServer = v),
                ),
                SwitchListTile(
                  contentPadding: EdgeInsets.zero,
                  title: const Text('Start the node when the app opens'),
                  value: _autostart,
                  onChanged: (v) => setState(() => _autostart = v),
                ),
                SwitchListTile(
                  contentPadding: EdgeInsets.zero,
                  title: const Text('Start hosted org sites with the node'),
                  value: _hostAutostart,
                  onChanged: (v) => setState(() => _hostAutostart = v),
                ),
                DropdownButtonFormField<String>(
                  key: const ValueKey('net-config-accept'),
                  initialValue: _accept,
                  items: const [
                    DropdownMenuItem(value: 'known', child: Text('known')),
                    DropdownMenuItem(value: 'anyone', child: Text('anyone')),
                  ],
                  onChanged: (v) => setState(() => _accept = v ?? 'known'),
                  decoration: const InputDecoration(
                    labelText: 'Accept connections from',
                  ),
                ),
                TextField(
                  controller: _bootstrap,
                  maxLines: 3,
                  decoration: const InputDecoration(
                    labelText: 'Bootstrap nodes',
                    hintText: 'one multiaddr per line',
                  ),
                ),
                TextField(
                  controller: _external,
                  maxLines: 3,
                  decoration: const InputDecoration(
                    labelText: 'External addresses',
                    hintText: 'host, host:port or a multiaddr, one per line',
                  ),
                ),
                DropdownButtonFormField<String>(
                  initialValue: _collabTransport,
                  items: const [
                    DropdownMenuItem(value: 'stream', child: Text('stream')),
                    DropdownMenuItem(value: 'poll', child: Text('poll')),
                  ],
                  onChanged: (v) =>
                      setState(() => _collabTransport = v ?? 'stream'),
                  decoration: const InputDecoration(
                    labelText: 'Collab transport',
                  ),
                ),
                TextField(
                  controller: _collabBind,
                  decoration: const InputDecoration(labelText: 'Collab bind'),
                ),
                TextField(
                  controller: _collabPort,
                  keyboardType: TextInputType.number,
                  decoration: const InputDecoration(labelText: 'Collab port'),
                ),
                TextField(
                  controller: _collabAdvertise,
                  maxLines: 2,
                  decoration: const InputDecoration(
                    labelText: 'Collab advertise',
                    hintText: 'host:port, one per line',
                  ),
                ),
              ],
            ),
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
          child: Align(
            alignment: Alignment.centerLeft,
            child: FilledButton(
              key: const ValueKey('net-settings-save'),
              onPressed: _save,
              child: const Text('Save settings'),
            ),
          ),
        ),
      ],
    );
  }
}

class _ConnectionsPane extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return ref
        .watch(netConnectionsProvider)
        .when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Padding(
            padding: const EdgeInsets.all(16),
            child: Text('$e', style: TextStyle(color: m.error)),
          ),
          data: (all) {
            if (all == null) {
              return const SizedBox.shrink();
            }
            return ListView(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
              children: [
                if (all.me != null) ...[
                  Text(
                    'You',
                    style: TextStyle(
                      color: m.header,
                      fontWeight: FontWeight.w800,
                      fontSize: 16,
                    ),
                  ),
                  ListTile(
                    contentPadding: EdgeInsets.zero,
                    title: Text(all.me!.name),
                    subtitle: Text(
                      '${all.me!.shortId} · ${all.me!.nodes} node(s)',
                      style: TextStyle(color: m.muted),
                    ),
                  ),
                  const SizedBox(height: 8),
                ],
                Text(
                  'Connections',
                  style: TextStyle(
                    color: m.header,
                    fontWeight: FontWeight.w800,
                    fontSize: 16,
                  ),
                ),
                if (all.connections.isEmpty)
                  Text(
                    'No connections yet. Someone you exchange keys with becomes a connection.',
                    style: TextStyle(color: m.muted),
                  )
                else
                  for (final c in all.connections)
                    ListTile(
                      contentPadding: EdgeInsets.zero,
                      title: Text(c.name),
                      subtitle: Text(
                        [
                          c.shortId,
                          c.trusted ? 'trusted' : 'untrusted',
                          '${c.nodes} node(s)',
                          for (final n in c.nodeList)
                            n.label.isEmpty ? n.shortId : n.label,
                        ].join(' · '),
                        style: TextStyle(color: m.muted),
                      ),
                      trailing: Wrap(
                        children: [
                          TextButton(
                            key: ValueKey('net-trust-${c.shortId}'),
                            onPressed: () async {
                              try {
                                final msg = await ref
                                    .read(apiProvider)
                                    .netConnectionTrust(c.userId, !c.trusted);
                                refreshNet(ref);
                                if (context.mounted) {
                                  _ok(context, msg);
                                }
                              } catch (e) {
                                if (context.mounted) {
                                  _ok(context, '$e', color: m.error);
                                }
                              }
                            },
                            child: Text(c.trusted ? 'Untrust' : 'Trust'),
                          ),
                          IconButton(
                            tooltip: 'Remove',
                            onPressed: () async {
                              if (!await confirmNet(
                                context,
                                'Remove connection',
                                'Remove the connection with ${c.name}?',
                              )) {
                                return;
                              }
                              try {
                                final msg = await ref
                                    .read(apiProvider)
                                    .netConnectionRemove(c.userId);
                                refreshNet(ref);
                                if (context.mounted) {
                                  _ok(context, msg);
                                }
                              } catch (e) {
                                if (context.mounted) {
                                  _ok(context, '$e', color: m.error);
                                }
                              }
                            },
                            icon: const Icon(Icons.delete_outline),
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

class _ExchangesPane extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return ref
        .watch(netExchangesProvider)
        .when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Padding(
            padding: const EdgeInsets.all(16),
            child: Text('$e', style: TextStyle(color: m.error)),
          ),
          data: (items) {
            final all = items ?? const <NetExchangeInfo>[];
            return ListView(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
              children: [
                Text(
                  'Key exchanges (Nodes). Invite tokens stay in Rust. Short codes are meant to be read aloud. Export or restore writes an age-encrypted backup; keys stay in Rust.',
                  style: TextStyle(color: m.muted),
                ),
                const SizedBox(height: 12),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    FilledButton.icon(
                      onPressed: () =>
                          showNetInviteOrConnectDialog(context, ref),
                      icon: const Icon(Icons.link),
                      label: const Text('Invite or connect…'),
                    ),
                    OutlinedButton(
                      key: const ValueKey('net-exchange-export'),
                      onPressed: () =>
                          showNetExchangeExportDialog(context, ref),
                      child: const Text('Export…'),
                    ),
                    OutlinedButton(
                      key: const ValueKey('net-exchange-restore'),
                      onPressed: () =>
                          showNetExchangeRestoreDialog(context, ref),
                      child: const Text('Restore…'),
                    ),
                  ],
                ),
                const SizedBox(height: 12),
                if (all.isEmpty)
                  Text(
                    'No key exchanges yet.',
                    style: TextStyle(color: m.muted),
                  )
                else
                  for (final x in all)
                    ListTile(
                      contentPadding: EdgeInsets.zero,
                      title: Text(x.title.isEmpty ? x.shortId : x.title),
                      subtitle: Text(
                        [
                          x.status,
                          if (x.label.isNotEmpty) x.label,
                          if (x.peerId.isNotEmpty) x.peerId,
                          if (x.shortCode != null) 'code ${x.shortCode}',
                          if (x.fingerprint != null) x.fingerprint!,
                        ].join(' · '),
                        style: TextStyle(color: m.muted),
                      ),
                      trailing: PopupMenuButton<String>(
                        onSelected: (v) async {
                          switch (v) {
                            case 'label':
                              await showNetExchangeLabelDialog(context, ref, x);
                            case 'verify':
                              await showNetExchangeVerifyDialog(
                                context,
                                ref,
                                x,
                              );
                            case 'list':
                              await showNetRemoteListDialog(context, ref, x);
                            case 'revoke':
                              if (!await confirmNet(
                                context,
                                'Disconnect',
                                'Disconnect from ${x.title.isEmpty ? x.shortId : x.title}? Keys are erased on both sides.',
                              )) {
                                return;
                              }
                              try {
                                final msg = await ref
                                    .read(apiProvider)
                                    .netExchangeRevoke(x.id);
                                refreshNet(ref);
                                if (context.mounted) {
                                  _ok(context, msg);
                                }
                              } catch (e) {
                                if (context.mounted) {
                                  _ok(context, '$e', color: m.error);
                                }
                              }
                          }
                        },
                        itemBuilder: (_) => const [
                          PopupMenuItem(value: 'label', child: Text('Label…')),
                          PopupMenuItem(
                            value: 'verify',
                            child: Text('Verify…'),
                          ),
                          PopupMenuItem(
                            value: 'list',
                            child: Text('List shares…'),
                          ),
                          PopupMenuItem(
                            value: 'revoke',
                            child: Text('Disconnect'),
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

class _SearchPane extends ConsumerStatefulWidget {
  @override
  ConsumerState<_SearchPane> createState() => _SearchPaneState();
}

class _SearchPaneState extends ConsumerState<_SearchPane> {
  String? _saved;
  bool _remoteOnly = false;
  final Set<String> _only = <String>{};
  List<NetSearchOrigin>? _hits;
  bool _busy = false;

  Future<void> _run() async {
    final name = _saved;
    if (name == null || name.isEmpty) {
      return;
    }
    setState(() => _busy = true);
    try {
      final hits = await ref
          .read(apiProvider)
          .netSearch(name, remoteOnly: _remoteOnly, only: _only.toList());
      if (mounted) {
        setState(() {
          _hits = hits;
          _busy = false;
        });
      }
    } catch (e) {
      if (mounted) {
        setState(() => _busy = false);
        _ok(context, '$e', color: context.muninn.error);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    final searches = ref.watch(searchesProvider).value ?? const <SearchInfo>[];
    if (_saved == null && searches.isNotEmpty) {
      _saved = searches.first.name;
    }
    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
      children: [
        Text(
          'Run a saved live search here and on peers (each peer searches only what it shares with you).',
          style: TextStyle(color: m.muted),
        ),
        const SizedBox(height: 12),
        if (searches.isEmpty)
          Text(
            'Save a live search in Projects first.',
            style: TextStyle(color: m.muted),
          )
        else
          DropdownButtonFormField<String>(
            key: const ValueKey('net-search-saved'),
            initialValue: _saved,
            items: [
              for (final s in searches)
                DropdownMenuItem(value: s.name, child: Text(s.name)),
            ],
            onChanged: (v) => setState(() => _saved = v),
            decoration: const InputDecoration(labelText: 'Live search'),
          ),
        CheckboxListTile(
          contentPadding: EdgeInsets.zero,
          title: const Text('Peers only (skip the local search)'),
          value: _remoteOnly,
          onChanged: (v) => setState(() => _remoteOnly = v ?? false),
        ),
        Builder(
          builder: (context) {
            final exchanges =
                ref.watch(netExchangesProvider).value ??
                const <NetExchangeInfo>[];
            if (exchanges.isEmpty) {
              return const SizedBox.shrink();
            }
            return Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'Only these exchanges (--only)',
                  style: TextStyle(color: context.muninn.muted),
                ),
                const SizedBox(height: 4),
                Wrap(
                  spacing: 8,
                  children: [
                    for (final x in exchanges)
                      FilterChip(
                        key: ValueKey('net-search-only-${x.id}'),
                        label: Text(x.title.isEmpty ? x.shortId : x.title),
                        selected:
                            _only.contains(x.id) || _only.contains(x.title),
                        onSelected: (on) => setState(() {
                          if (on) {
                            _only.add(x.id);
                          } else {
                            _only.remove(x.id);
                            _only.remove(x.title);
                          }
                        }),
                      ),
                  ],
                ),
                const SizedBox(height: 8),
              ],
            );
          },
        ),
        Align(
          alignment: Alignment.centerLeft,
          child: FilledButton(
            key: const ValueKey('net-search-run'),
            onPressed: searches.isEmpty || _busy ? null : _run,
            child: Text(_busy ? 'Searching…' : 'Run search'),
          ),
        ),
        const SizedBox(height: 16),
        if (_hits != null)
          for (final o in _hits!) ...[
            Text(
              o.origin,
              style: TextStyle(color: m.header, fontWeight: FontWeight.w800),
            ),
            if (o.error != null)
              Text(o.error!, style: TextStyle(color: m.error)),
            if (o.lines.isEmpty && o.error == null)
              Text('No matches.', style: TextStyle(color: m.muted)),
            for (final line in o.lines)
              Padding(
                padding: const EdgeInsets.only(bottom: 4),
                child: Text(line),
              ),
            if (o.truncated)
              Text('truncated', style: TextStyle(color: m.muted)),
            const SizedBox(height: 12),
          ],
      ],
    );
  }
}

class _ShardsPane extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return ref
        .watch(netShardsProvider)
        .when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Padding(
            padding: const EdgeInsets.all(16),
            child: Text('$e', style: TextStyle(color: m.error)),
          ),
          data: (all) {
            if (all == null) {
              return const SizedBox.shrink();
            }
            return ListView(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
              children: [
                Text(
                  'Dark shards split your master key across trusted nodes. The key file and shard password stay in Rust; sealed blobs never leave it. Restoring a key when this database cannot open stays in the CLI.',
                  style: TextStyle(color: m.muted),
                ),
                const SizedBox(height: 12),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    FilledButton.icon(
                      key: const ValueKey('net-shard-entrust'),
                      onPressed: () => showNetShardEntrustDialog(context, ref),
                      icon: const Icon(Icons.safety_check),
                      label: const Text('Entrust…'),
                    ),
                    OutlinedButton(
                      key: const ValueKey('net-shard-check'),
                      onPressed: () => showNetShardCheckDialog(context, ref),
                      child: const Text('Check restore'),
                    ),
                  ],
                ),
                const SizedBox(height: 16),
                Text(
                  'Entrusted',
                  style: TextStyle(
                    color: m.header,
                    fontWeight: FontWeight.w800,
                    fontSize: 16,
                  ),
                ),
                if (all.entrusted.isEmpty)
                  Text(
                    'No dark shards entrusted.',
                    style: TextStyle(color: m.muted),
                  )
                else
                  for (final e in all.entrusted)
                    ListTile(
                      contentPadding: EdgeInsets.zero,
                      title: Text(
                        '${e.exchange.isEmpty ? e.setId : e.exchange} · shard ${e.index}/${e.total}',
                      ),
                      subtitle: Text(
                        'any ${e.threshold} of ${e.total} · ${e.status}',
                        style: TextStyle(color: m.muted),
                      ),
                    ),
                const SizedBox(height: 16),
                Text(
                  'Held',
                  style: TextStyle(
                    color: m.header,
                    fontWeight: FontWeight.w800,
                    fontSize: 16,
                  ),
                ),
                if (all.held.isEmpty)
                  Text('No dark shards held.', style: TextStyle(color: m.muted))
                else
                  for (final h in all.held)
                    ListTile(
                      contentPadding: EdgeInsets.zero,
                      title: Text('${h.owner} · shard ${h.index}/${h.total}'),
                      subtitle: Text(
                        'any ${h.threshold} of ${h.total} · ${h.shortId}',
                        style: TextStyle(color: m.muted),
                      ),
                      trailing: IconButton(
                        tooltip: 'Drop',
                        onPressed: () async {
                          if (!await confirmNet(
                            context,
                            'Drop shard',
                            'Drop the shard you hold for ${h.owner}?',
                          )) {
                            return;
                          }
                          try {
                            final msg = await ref
                                .read(apiProvider)
                                .netShardDrop(h.id);
                            refreshNet(ref);
                            if (context.mounted) {
                              _ok(context, msg);
                            }
                          } catch (e) {
                            if (context.mounted) {
                              _ok(context, '$e', color: m.error);
                            }
                          }
                        },
                        icon: const Icon(Icons.delete_outline),
                      ),
                    ),
              ],
            );
          },
        );
  }
}
