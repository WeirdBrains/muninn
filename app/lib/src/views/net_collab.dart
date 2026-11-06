/// TUI `W`: stored peer messages and live sessions. Sending a message or
/// joining a live session needs the node worker and stays in the TUI.
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

class CollabPane extends ConsumerWidget {
  const CollabPane({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final messages = ref.watch(peerMessagesProvider);
    final sessions = ref.watch(liveSessionsProvider);
    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
      children: [
        Text(
          'Peers, messages and live sessions (TUI W). This lists what the store already holds. Sending a message or hosting/joining a live session needs the node worker and stays in the TUI.',
          style: TextStyle(color: m.muted),
        ),
        const SizedBox(height: 16),
        Row(
          children: [
            Text('Messages', style: TextStyle(color: m.header, fontSize: 16)),
            const Spacer(),
            TextButton(
              key: const ValueKey('collab-mark-read'),
              onPressed: () async {
                try {
                  final msg = await ref
                      .read(apiProvider)
                      .peerMessagesMarkRead();
                  refreshNet(ref);
                  if (context.mounted) _ok(context, msg);
                } catch (e) {
                  if (context.mounted) {
                    _ok(context, '$e', color: m.error);
                  }
                }
              },
              child: const Text('Mark read'),
            ),
          ],
        ),
        messages.when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Text('$e', style: TextStyle(color: m.error)),
          data: (items) {
            final all = items ?? const <PeerMessageInfo>[];
            if (all.isEmpty) {
              return Text('No messages yet.', style: TextStyle(color: m.muted));
            }
            return Column(
              children: [
                for (final msg in all)
                  ListTile(
                    key: ValueKey('collab-msg-${msg.id}'),
                    contentPadding: EdgeInsets.zero,
                    title: Text(msg.text),
                    subtitle: Text(
                      [
                        msg.direction,
                        if (msg.peerLabel.isNotEmpty)
                          msg.peerLabel
                        else
                          msg.peer,
                        if (msg.about.isNotEmpty) msg.about,
                        if (msg.read) 'read' else 'unread',
                        if (msg.error.isNotEmpty) msg.error,
                      ].join(' · '),
                      style: TextStyle(color: msg.read ? m.muted : m.accent),
                    ),
                  ),
              ],
            );
          },
        ),
        const SizedBox(height: 16),
        Text('Live', style: TextStyle(color: m.header, fontSize: 16)),
        sessions.when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Text('$e', style: TextStyle(color: m.error)),
          data: (items) {
            final all = items ?? const <LiveSessionInfo>[];
            if (all.isEmpty) {
              return Text(
                'No live sessions.',
                style: TextStyle(color: m.muted),
              );
            }
            return Column(
              children: [
                for (final s in all)
                  ListTile(
                    key: ValueKey('collab-live-${s.id}'),
                    contentPadding: EdgeInsets.zero,
                    title: Text(s.title.isNotEmpty ? s.title : s.id),
                    subtitle: Text(
                      [
                        s.kind,
                        s.role,
                        s.status,
                        if (s.hostPeer.isNotEmpty) s.hostPeer,
                      ].join(' · '),
                      style: TextStyle(color: m.muted),
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
