/// Net: `mnnn net status|peers` plus start/stop, connect, invites, relays and
/// sharing. The node identity stays in Rust; invite links hold no secret.
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
