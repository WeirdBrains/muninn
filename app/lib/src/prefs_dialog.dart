/// File ▸ Preferences…: the theme, mode, time range and open dataset, saved in
/// `preferences.toml` (shared with the TUI). Changes go through the same providers
/// the Theme menu, time range picker and Data view already use.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'api.dart';
import 'state.dart';
import 'theme/muninn_theme.dart';
import 'time_range.dart';

Future<void> showPreferencesDialog(BuildContext context, WidgetRef ref) =>
    showDialog(context: context, builder: (ctx) => const PreferencesDialog());

class PreferencesDialog extends ConsumerWidget {
  const PreferencesDialog({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final names = ref.watch(themeNamesProvider).value ?? const <String>[];
    final sel = ref.watch(themeSelectionProvider);
    final range = ref.watch(timeRangeProvider);
    final dataset = ref.watch(selectedDatasetProvider);
    return AlertDialog(
      title: Row(
        children: [
          Expanded(
            child: Text(
              'Preferences',
              style: TextStyle(color: m.header, fontWeight: FontWeight.w800),
            ),
          ),
          IconButton(
            tooltip: 'Close',
            icon: const Text('[X]'),
            onPressed: () => Navigator.pop(context),
          ),
        ],
      ),
      content: SizedBox(
        width: 480,
        child: SingleChildScrollView(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text(
                'Saved in preferences.toml, shared with the TUI (`mnnn theme` and the time range).',
                style: TextStyle(color: m.muted),
              ),
              const SizedBox(height: 16),
              Text(
                'Theme',
                style: TextStyle(color: m.header, fontWeight: FontWeight.w700),
              ),
              const SizedBox(height: 8),
              SegmentedButton<String>(
                segments: const [
                  ButtonSegment(value: 'system', label: Text('System')),
                  ButtonSegment(value: 'dark', label: Text('Dark')),
                  ButtonSegment(value: 'light', label: Text('Light')),
                ],
                selected: {sel.mode},
                onSelectionChanged: (s) =>
                    ref.read(themeSelectionProvider.notifier).setMode(s.first),
              ),
              const SizedBox(height: 8),
              ConstrainedBox(
                constraints: const BoxConstraints(maxHeight: 220),
                child: ListView(
                  shrinkWrap: true,
                  children: [
                    for (final n in names)
                      ListTile(
                        key: ValueKey('prefs-theme-$n'),
                        dense: true,
                        title: Text(n),
                        leading: Icon(
                          n == sel.name
                              ? Icons.radio_button_checked
                              : Icons.radio_button_unchecked,
                          size: 18,
                          color: m.accent,
                        ),
                        onTap: () =>
                            ref.read(themeSelectionProvider.notifier).set(n),
                      ),
                  ],
                ),
              ),
              const SizedBox(height: 16),
              Text(
                'Time range',
                style: TextStyle(color: m.header, fontWeight: FontWeight.w700),
              ),
              const SizedBox(height: 4),
              Text(_rangeLabel(range), style: TextStyle(color: m.muted)),
              const SizedBox(height: 8),
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  OutlinedButton(
                    key: const ValueKey('prefs-range'),
                    onPressed: () => showTimeRangePicker(context, ref),
                    child: const Text('Change…'),
                  ),
                  if (!range.isAll)
                    OutlinedButton(
                      key: const ValueKey('prefs-range-clear'),
                      onPressed: () =>
                          ref.read(timeRangeProvider.notifier).state =
                              TimeRange.all,
                      child: const Text('Clear'),
                    ),
                ],
              ),
              const SizedBox(height: 16),
              Text(
                'Open dataset',
                style: TextStyle(color: m.header, fontWeight: FontWeight.w700),
              ),
              const SizedBox(height: 4),
              Text(
                dataset ?? 'None (pick one in Data)',
                style: TextStyle(color: m.muted),
              ),
              if (dataset != null) ...[
                const SizedBox(height: 8),
                Align(
                  alignment: Alignment.centerLeft,
                  child: OutlinedButton(
                    key: const ValueKey('prefs-dataset-clear'),
                    onPressed: () =>
                        ref.read(selectedDatasetProvider.notifier).state = null,
                    child: const Text('Clear'),
                  ),
                ),
              ],
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

String _rangeLabel(TimeRange r) {
  if (r.isAll) return 'All';
  final from = r.from == null ? '…' : '${fmtUtc(r.from!)} UTC';
  final to = r.to == null ? '…' : '${fmtUtc(r.to!)} UTC';
  return '$from → $to';
}
