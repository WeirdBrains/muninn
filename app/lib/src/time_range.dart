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

/// Parse the TUI prompt format (`YYYY-MM-DD [HH:MM]`, optional seconds) or an
/// epoch number, as `parse_ts` does for `{` `}` `@`.
int? parseUtcMs(String s) {
  final t = s.trim().replaceAll(RegExp(r' UTC$'), '').replaceAll(RegExp(r'Z$'), '');
  if (t.isEmpty) return null;
  final asInt = int.tryParse(t);
  if (asInt != null) {
    return asInt.abs() < 100000000000 ? asInt * 1000 : asInt;
  }
  final asDouble = double.tryParse(t);
  if (asDouble != null && asDouble.isFinite) {
    return asDouble.abs() < 1e11
        ? (asDouble * 1000).round()
        : asDouble.round();
  }
  final m = RegExp(
    r'^(\d{4})-(\d{2})-(\d{2})(?:[ T](\d{2}):(\d{2})(?::(\d{2}))?)?$',
  ).firstMatch(t);
  if (m == null) return null;
  try {
    return DateTime.utc(
      int.parse(m[1]!),
      int.parse(m[2]!),
      int.parse(m[3]!),
      int.parse(m[4] ?? '0'),
      int.parse(m[5] ?? '0'),
      int.parse(m[6] ?? '0'),
    ).millisecondsSinceEpoch;
  } catch (_) {
    return null;
  }
}

/// Calendar + time pickers used by the time-range dialog and by chart `[` `]`.
Future<int?> pickUtcMs(BuildContext context, int? current) async {
  final now = DateTime.now().toUtc();
  final cur = current == null
      ? DateTime.utc(now.year, now.month, now.day)
      : DateTime.fromMillisecondsSinceEpoch(current, isUtc: true);
  final day = await showDatePicker(
    context: context,
    initialDate: cur,
    firstDate: DateTime.utc(2009),
    lastDate: DateTime.utc(now.year + 1, 12, 31),
  );
  if (day == null || !context.mounted) return null;
  final t = await showTimePicker(
    context: context,
    initialTime: TimeOfDay(hour: cur.hour, minute: cur.minute),
    helpText: 'Time (UTC)',
    builder: (c, child) => MediaQuery(
      data: MediaQuery.of(c).copyWith(alwaysUse24HourFormat: true),
      child: child!,
    ),
  );
  if (!context.mounted) return null;
  final tod = t ?? TimeOfDay(hour: cur.hour, minute: cur.minute);
  return DateTime.utc(
    day.year,
    day.month,
    day.day,
    tod.hour,
    tod.minute,
  ).millisecondsSinceEpoch;
}

/// Typed date prompt matching TUI `{` `}` `@` (`YYYY-MM-DD [HH:MM]`).
Future<int?> showUtcDatePrompt(
  BuildContext context, {
  required String title,
}) =>
    showDialog<int>(
      context: context,
      builder: (_) => _UtcDatePrompt(title: title),
    );

class _UtcDatePrompt extends StatefulWidget {
  const _UtcDatePrompt({required this.title});
  final String title;
  @override
  State<_UtcDatePrompt> createState() => _UtcDatePromptState();
}

class _UtcDatePromptState extends State<_UtcDatePrompt> {
  final _ctl = TextEditingController();
  String? _error;

  @override
  void dispose() {
    _ctl.dispose();
    super.dispose();
  }

  void _submit() {
    final ms = parseUtcMs(_ctl.text);
    if (ms == null) {
      setState(() => _error = "can't read date `${_ctl.text}`");
      return;
    }
    Navigator.pop(context, ms);
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      key: const ValueKey<String>('utc-date-prompt'),
      title: Text(widget.title),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          TextField(
            key: const ValueKey<String>('utc-date-input'),
            controller: _ctl,
            autofocus: true,
            decoration: const InputDecoration(
              hintText: 'YYYY-MM-DD [HH:MM]',
            ),
            onSubmitted: (_) => _submit(),
          ),
          if (_error != null)
            Padding(
              padding: const EdgeInsets.only(top: 8),
              child: Text(
                _error!,
                style: TextStyle(color: context.muninn.error),
              ),
            ),
        ],
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey<String>('utc-date-apply'),
          onPressed: _submit,
          child: const Text('Apply'),
        ),
      ],
    );
  }
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
    final picked = await pickUtcMs(context, current);
    return picked ?? current;
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
