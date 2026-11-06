/// Net (read-only): `mnnn net status` and `mnnn net peers`. Start/stop, connect,
/// invite and keys stay in the TUI or CLI. The store is opened read-only; no
/// key material is sent to Dart.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../state.dart';
import '../theme/muninn_theme.dart';
import '../time_range.dart';
import 'unlock.dart';

class NetView extends ConsumerWidget {
  const NetView({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return Unlocked(
      child: ref
          .watch(netOverviewProvider)
          .when(
            loading: () => const LinearProgressIndicator(),
            error: (e, _) => Padding(
              padding: const EdgeInsets.all(16),
              child: Text('$e', style: TextStyle(color: m.error)),
            ),
            data: (o) {
              if (o == null) return const SizedBox.shrink();
              return ListView(
                padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
                children: [
                  _NodeStatus(o.status),
                  const SizedBox(height: 20),
                  Text(
                    'Peers',
                    style: TextStyle(
                      color: m.header,
                      fontWeight: FontWeight.w800,
                      fontSize: 16,
                    ),
                  ),
                  const SizedBox(height: 8),
                  if (o.peers.isEmpty)
                    Text(
                      'No peers yet: start the node (`mnnn net start`) or `mnnn net connect ADDR`',
                      style: TextStyle(color: m.muted),
                    )
                  else
                    for (final p in o.peers) _PeerTile(p),
                  const SizedBox(height: 24),
                  Text(
                    'Start or stop the node with `mnnn net start` / `mnnn net stop`, or S / X in the TUI. Connecting and invites stay there too.',
                    style: TextStyle(color: m.muted),
                  ),
                ],
              );
            },
          ),
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
        : 'no (start with `mnnn net start`)';
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

class _PeerTile extends StatelessWidget {
  const _PeerTile(this.p);
  final NetPeerInfo p;

  Color _linkColor(MuninnColors m) => switch (p.link) {
    'connected' => m.success,
    'requested' || 'pending' => m.warning,
    'failed' => m.error,
    _ => m.muted,
  };

  @override
  Widget build(BuildContext context) {
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
    );
  }
}
