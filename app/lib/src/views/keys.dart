/// Keys: `mnnn keys status` plus init, rotate, rekey, export, recover and shards.
/// Private material stays in Rust; recovery words and shards appear only in a
/// one-time reveal sheet (one line at a time from a Rust session).
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../state.dart';
import '../theme/muninn_theme.dart';
import 'keys_dialogs.dart';
import 'shared_actions.dart';

class KeysView extends ConsumerWidget {
  const KeysView({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return ref
        .watch(keysStatusProvider)
        .when(
          loading: () => const LinearProgressIndicator(),
          error: (e, _) => Padding(
            padding: const EdgeInsets.all(16),
            child: Text('$e', style: TextStyle(color: m.error)),
          ),
          data: (s) => ListView(
            padding: const EdgeInsets.fromLTRB(16, 12, 16, 24),
            children: [
              Text(
                'Keys',
                style: TextStyle(
                  color: m.header,
                  fontWeight: FontWeight.w800,
                  fontSize: 16,
                ),
              ),
              const SizedBox(height: 12),
              if (!s.initialized) ...[
                Text(
                  key: const ValueKey('keys-uninitialized'),
                  'Keys are not initialized.',
                  style: TextStyle(color: m.muted),
                ),
                const SizedBox(height: 8),
                Align(
                  alignment: Alignment.centerLeft,
                  child: FilledButton.icon(
                    key: const ValueKey('keys-init'),
                    onPressed: () => showKeysInitDialog(context, ref),
                    icon: const Icon(Icons.vpn_key),
                    label: const Text('Initialize…'),
                  ),
                ),
              ] else ...[
                _row(m, 'keys-db-id', 'database id', s.dbId),
                _row(m, 'keys-database', 'database', s.database),
                _row(m, 'keys-service', 'service key', 'v${s.serviceVersion}'),
                const SizedBox(height: 8),
                for (final k in s.services)
                  Padding(
                    padding: const EdgeInsets.only(left: 8, bottom: 4),
                    child: Text(
                      'v${k.version}  ${k.retired ? 'retired' : 'active'}',
                      style: TextStyle(color: m.fg),
                    ),
                  ),
                const SizedBox(height: 16),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    OutlinedButton(
                      key: const ValueKey('keys-rotate'),
                      onPressed: () => showKeysRotateDialog(context, ref),
                      child: const Text('Rotate service key'),
                    ),
                    OutlinedButton(
                      key: const ValueKey('keys-rekey'),
                      onPressed: () => showKeysRekeyDialog(context, ref),
                      child: const Text('Rekey database'),
                    ),
                    OutlinedButton(
                      key: const ValueKey('keys-export'),
                      onPressed: () => showKeysExportDialog(context, ref),
                      child: const Text('Export…'),
                    ),
                    OutlinedButton(
                      key: const ValueKey('keys-recover-words'),
                      onPressed: () => showKeysRecoverWordsDialog(context, ref),
                      child: const Text('Recover from words'),
                    ),
                    OutlinedButton(
                      key: const ValueKey('keys-recover-shards'),
                      onPressed: () =>
                          showKeysRecoverShardsDialog(context, ref),
                      child: const Text('Recover from shards'),
                    ),
                    OutlinedButton(
                      key: const ValueKey('keys-shard-backup'),
                      onPressed: () =>
                          showShardBackupRestoreDialog(context, ref),
                      child: const Text('Restore from a shard backup'),
                    ),
                    OutlinedButton(
                      key: const ValueKey('keys-reissue'),
                      onPressed: () =>
                          showKeysReissueShardsDialog(context, ref),
                      child: const Text('Re-issue shards'),
                    ),
                  ],
                ),
              ],
              const SizedBox(height: 24),
              Text(
                'Recovery words and shards are shown once from a Rust-held session and then wiped. The master key, wrapped keys and raw key bytes never sit in Dart.',
                style: TextStyle(color: m.muted),
              ),
            ],
          ),
        );
  }
}

Widget _row(MuninnColors m, String key, String label, String value) {
  return Padding(
    padding: const EdgeInsets.only(bottom: 6),
    child: Row(
      key: ValueKey(key),
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SizedBox(
          width: 140,
          child: Text(label, style: TextStyle(color: m.muted)),
        ),
        Expanded(
          child: Text(value, style: TextStyle(color: m.fg)),
        ),
      ],
    ),
  );
}
