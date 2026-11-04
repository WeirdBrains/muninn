import 'package:flutter/material.dart';

import '../theme/muninn_theme.dart';

/// A section the app doesn't show yet: points at the CLI and TUI equivalents.
class PlaceholderView extends StatelessWidget {
  const PlaceholderView({super.key, required this.title, required this.cli, required this.icon});
  final String title, cli;
  final IconData icon;

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          Icon(icon, size: 56, color: m.accent),
          const SizedBox(height: 12),
          Text(title, style: Theme.of(context).textTheme.headlineMedium?.copyWith(color: m.header, fontWeight: FontWeight.w800)),
          const SizedBox(height: 8),
          Text('Coming to the app. Today: `$cli` or `mnnn tui`.', textAlign: TextAlign.center, style: TextStyle(color: m.muted)),
        ]),
      ),
    );
  }
}
