/// The global time range, as in the TUI (`T`, Data ▸ Filters ▸ Time range…): it scopes every
/// view. A bar of chips sits above every section; the picker sets *from* and *to* (UTC), and
/// either end can be left open.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'api.dart';
import 'state.dart';
import 'theme/muninn_theme.dart';

/// `2024-01-01 00:00` (UTC), the TUI's format.
String fmtUtc(int ms) {
  final d = DateTime.fromMillisecondsSinceEpoch(ms, isUtc: true);
  String two(int n) => n.toString().padLeft(2, '0');
  return '${d.year}-${two(d.month)}-${two(d.day)} ${two(d.hour)}:${two(d.minute)}';
}

class RangeBar extends ConsumerWidget {
  const RangeBar({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final r = ref.watch(timeRangeProvider);
    final m = context.muninn;
    final set = ref.read(timeRangeProvider.notifier);
    return Padding(
      padding: const EdgeInsets.fromLTRB(12, 6, 12, 0),
      child: Wrap(spacing: 8, runSpacing: 4, crossAxisAlignment: WrapCrossAlignment.center, children: [
        ActionChip(
          avatar: Icon(Icons.date_range, size: 18, color: m.accent),
          label: Text(r.isAll ? 'Time range: all' : 'Time range'),
          tooltip: 'Set the time range (T)',
          onPressed: () => showTimeRangePicker(context, ref),
        ),
        if (r.from != null)
          InputChip(
            label: Text('from ${fmtUtc(r.from!)} UTC'),
            onPressed: () => showTimeRangePicker(context, ref),
            onDeleted: () => set.state = r.withFrom(null),
            deleteButtonTooltipMessage: 'Remove from',
          ),
        if (r.to != null)
          InputChip(
            label: Text('to ${fmtUtc(r.to!)} UTC'),
            onPressed: () => showTimeRangePicker(context, ref),
            onDeleted: () => set.state = r.withTo(null),
            deleteButtonTooltipMessage: 'Remove to',
          ),
      ]),
    );
  }
}

Future<void> showTimeRangePicker(BuildContext context, WidgetRef ref) async {
  final r = await showDialog<TimeRange>(
    context: context,
    builder: (_) => _RangeDialog(initial: ref.read(timeRangeProvider)),
  );
  if (r != null) ref.read(timeRangeProvider.notifier).state = r;
}

class _RangeDialog extends StatefulWidget {
  const _RangeDialog({required this.initial});
  final TimeRange initial;
  @override
  State<_RangeDialog> createState() => _RangeDialogState();
}

class _RangeDialogState extends State<_RangeDialog> {
  late TimeRange r = widget.initial;

  Future<int?> _pick(int? current) async {
    final now = DateTime.now().toUtc();
    final cur = current == null ? DateTime.utc(now.year, now.month, now.day) : DateTime.fromMillisecondsSinceEpoch(current, isUtc: true);
    final day = await showDatePicker(context: context, initialDate: cur, firstDate: DateTime.utc(2009), lastDate: DateTime.utc(now.year + 1, 12, 31));
    if (day == null || !mounted) return current;
    final t = await showTimePicker(
      context: context,
      initialTime: TimeOfDay(hour: cur.hour, minute: cur.minute),
      helpText: 'Time (UTC)',
      builder: (c, child) => MediaQuery(data: MediaQuery.of(c).copyWith(alwaysUse24HourFormat: true), child: child!),
    );
    final tod = t ?? TimeOfDay(hour: cur.hour, minute: cur.minute);
    return DateTime.utc(day.year, day.month, day.day, tod.hour, tod.minute).millisecondsSinceEpoch;
  }

  Widget _end(String label, int? v, ValueChanged<int?> set) => Padding(
        padding: const EdgeInsets.symmetric(vertical: 4),
        child: Row(children: [
          SizedBox(width: 48, child: Text(label)),
          Expanded(
            child: OutlinedButton.icon(
              style: OutlinedButton.styleFrom(minimumSize: const Size(0, 48)),
              icon: const Icon(Icons.edit_calendar),
              label: Text(v == null ? 'open' : '${fmtUtc(v)} UTC'),
              onPressed: () async => set(await _pick(v)),
            ),
          ),
          IconButton(tooltip: 'Leave $label open (x)', icon: const Icon(Icons.close), onPressed: v == null ? null : () => set(null)),
        ]),
      );

  @override
  Widget build(BuildContext context) {
    final bad = r.from != null && r.to != null && r.from! > r.to!;
    return AlertDialog(
      title: Row(children: [
        const Expanded(child: Text('Time range')),
        IconButton(tooltip: 'Close', icon: const Text('[X]'), onPressed: () => Navigator.pop(context)),
      ]),
      content: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        _end('From', r.from, (v) => setState(() => r = r.withFrom(v))),
        _end('To', r.to, (v) => setState(() => r = r.withTo(v))),
        if (bad) Text('From is after To', style: TextStyle(color: context.muninn.error)),
        Text('Applies to every view, like T in the TUI.', style: TextStyle(color: context.muninn.muted, fontSize: 12)),
      ]),
      actions: [
        TextButton(onPressed: () => Navigator.pop(context, TimeRange.all), child: const Text('Clear')),
        FilledButton(onPressed: bad ? null : () => Navigator.pop(context, r), child: const Text('Apply')),
      ],
    );
  }
}
