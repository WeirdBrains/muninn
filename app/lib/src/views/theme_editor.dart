/// Theme ▸ Edit current…, New from current…, Import… and Export….
library;

import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../state.dart';
import '../theme/muninn_theme.dart';

Future<void> showThemeEditor(
  BuildContext context,
  WidgetRef ref, {
  bool saveAs = false,
}) async {
  final sel = ref.read(themeSelectionProvider);
  final draft = await ref.read(apiProvider).themeDraft(sel.name);
  if (!context.mounted) return;
  final name = TextEditingController(
    text: saveAs ? '${draft.name}-custom' : draft.name,
  );
  final display = TextEditingController(
    text: saveAs ? '${draft.displayName} custom' : draft.displayName,
  );
  var variant = 'dark';
  final darkCtl = {
    for (final e in draft.dark.entries)
      e.key: TextEditingController(text: e.value),
  };
  final lightCtl = {
    for (final e in draft.light.entries)
      e.key: TextEditingController(text: e.value),
  };
  final saved = await showDialog<bool>(
    context: context,
    builder: (ctx) => StatefulBuilder(
      builder: (ctx, set) {
        final m = ctx.muninn;
        final roles = variant == 'light' ? lightCtl : darkCtl;
        final keys = roles.keys.toList()..sort();
        return AlertDialog(
          title: Text(
            saveAs ? 'New from current' : 'Edit theme',
            style: TextStyle(color: m.header),
          ),
          content: SizedBox(
            width: 420,
            height: 420,
            child: Column(
              children: [
                TextField(
                  key: const ValueKey('theme-name'),
                  controller: name,
                  enabled: saveAs,
                  decoration: const InputDecoration(labelText: 'Name'),
                ),
                TextField(
                  controller: display,
                  decoration: const InputDecoration(labelText: 'Display name'),
                ),
                const SizedBox(height: 8),
                SegmentedButton<String>(
                  segments: const [
                    ButtonSegment(value: 'dark', label: Text('Dark')),
                    ButtonSegment(value: 'light', label: Text('Light')),
                  ],
                  selected: {variant},
                  onSelectionChanged: (s) => set(() => variant = s.first),
                ),
                Expanded(
                  child: ListView(
                    children: [
                      for (final role in keys)
                        TextField(
                          key: ValueKey('theme-role-$role'),
                          controller: roles[role],
                          decoration: InputDecoration(labelText: role),
                        ),
                    ],
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
              key: const ValueKey('theme-save'),
              onPressed: () => Navigator.pop(ctx, true),
              child: const Text('Save'),
            ),
          ],
        );
      },
    ),
  );
  if (saved == true) {
    final base = draft.base.isNotEmpty
        ? draft.base
        : (saveAs ? draft.name : '');
    Map<String, String> hexes(Map<String, TextEditingController> cs) => {
      for (final e in cs.entries) e.key: e.value.text.trim(),
    };
    await ref
        .read(apiProvider)
        .themeSave(
          name: name.text.trim(),
          displayName: display.text.trim(),
          base: base,
          dark: hexes(darkCtl),
          light: hexes(lightCtl),
        );
    ref.invalidate(themeNamesProvider);
    if (saveAs) {
      ref.read(themeSelectionProvider.notifier).set(name.text.trim());
    }
  }
  name.dispose();
  display.dispose();
  for (final c in [...darkCtl.values, ...lightCtl.values]) {
    c.dispose();
  }
}

Future<void> importTheme(BuildContext context, WidgetRef ref) async {
  final picked = await FilePicker.platform.pickFiles(
    type: FileType.custom,
    allowedExtensions: ['toml'],
  );
  final path = picked?.files.single.path;
  if (path == null) return;
  final name = await ref
      .read(apiProvider)
      .themeImport(await File(path).readAsString());
  ref.invalidate(themeNamesProvider);
  ref.read(themeSelectionProvider.notifier).set(name);
}

Future<void> exportTheme(BuildContext context, WidgetRef ref) async {
  final sel = ref.read(themeSelectionProvider);
  final text = await ref.read(apiProvider).themeExport(sel.name);
  final path = await FilePicker.platform.saveFile(fileName: '${sel.name}.toml');
  if (path == null) return;
  await File(path).writeAsString(text);
}
