/// File ▸ Backup. Create writes an archive here. Restore stays on the command
/// line, the same as the TUI (it replaces the open database).
library;

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../state.dart';
import '../theme/muninn_theme.dart';

Future<void> showBackupCreate(BuildContext context, WidgetRef ref) async {
  final dir = TextEditingController();
  final password = TextEditingController();
  final m = context.muninn;
  final go = await showDialog<bool>(
    context: context,
    builder: (ctx) => AlertDialog(
      title: Text('Create backup', style: TextStyle(color: m.header)),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Text(
              'Writes the database (still encrypted), keys/, preferences and themes. Leave the password empty for an unencrypted archive.',
            ),
            TextField(
              key: const ValueKey('backup-dir'),
              controller: dir,
              decoration: const InputDecoration(labelText: 'Directory'),
            ),
            Align(
              alignment: Alignment.centerLeft,
              child: TextButton(
                onPressed: () async {
                  final picked = await FilePicker.platform.getDirectoryPath();
                  if (picked != null) dir.text = picked;
                },
                child: const Text('Choose…'),
              ),
            ),
            TextField(
              key: const ValueKey('backup-password'),
              controller: password,
              obscureText: true,
              decoration: const InputDecoration(
                labelText: 'Archive password (optional)',
              ),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(ctx, false),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey('backup-create'),
          onPressed: () => Navigator.pop(ctx, true),
          child: const Text('Create'),
        ),
      ],
    ),
  );
  final out = dir.text.trim();
  final pw = password.text;
  dir.dispose();
  password.dispose();
  if (go != true || out.isEmpty || !context.mounted) return;
  try {
    final wrote = await ref.read(apiProvider).backupCreate(out, password: pw);
    if (context.mounted) {
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(wrote)));
    }
  } catch (e) {
    if (context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text('$e')));
    }
  }
}

Future<void> showBackupRestore(BuildContext context) => showDialog<void>(
  context: context,
  builder: (ctx) => AlertDialog(
    title: const Text('Restore'),
    content: const Text(
      'Restoring replaces the open database, so it runs from the command line with the app closed.\n\nRun:\n  mnnn backup restore --from <archive or dir>',
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(ctx),
        child: const Text('Close'),
      ),
    ],
  ),
);
