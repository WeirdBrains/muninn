/// About Muninn: the same text as the TUI's About box (muninn_core::about over FFI).
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'logo.dart';
import 'state.dart';
import 'theme/muninn_theme.dart';

Future<void> showMuninnAbout(BuildContext context, WidgetRef ref) {
  final a = ref.read(apiProvider).about();
  return showDialog(
    context: context,
    builder: (ctx) {
      final m = ctx.muninn;
      return AlertDialog(
        title: Row(children: [
          const RavenLogo(size: 56),
          const SizedBox(width: 12),
          Expanded(child: Text('${a.name} v${a.version}', style: TextStyle(color: m.header, fontWeight: FontWeight.w800))),
          IconButton(tooltip: 'Close', icon: const Text('[X]'), onPressed: () => Navigator.pop(ctx)),
        ]),
        content: SingleChildScrollView(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
            Text(a.tagline),
            Text(a.motto, style: TextStyle(color: m.accent, fontStyle: FontStyle.italic)),
            const SizedBox(height: 12),
            SelectableText(a.homepage, style: TextStyle(color: m.info)),
            SelectableText(a.repository, style: TextStyle(color: m.muted)),
            const SizedBox(height: 12),
            Text('${a.publisher} · ${a.license} · ${a.copyright}', style: TextStyle(color: m.muted)),
            for (final c in a.credits) Text('• $c', style: TextStyle(color: m.muted, fontSize: 12)),
          ]),
        ),
      );
    },
  );
}
