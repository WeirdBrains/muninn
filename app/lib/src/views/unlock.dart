/// Unlock the store with the Muninn password (read-only; the key stays in Rust memory).
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../state.dart';
import '../theme/muninn_theme.dart';

class UnlockCard extends ConsumerStatefulWidget {
  const UnlockCard({super.key});
  @override
  ConsumerState<UnlockCard> createState() => _UnlockCardState();
}

class _UnlockCardState extends ConsumerState<UnlockCard> {
  final _c = TextEditingController();
  String? _error;
  bool _busy = false;

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  Future<void> _go() async {
    if (_c.text.isEmpty || _busy) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await ref.read(apiProvider).unlock(_c.text);
      _c.clear();
      ref.read(unlockedProvider.notifier).state = true;
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => Card(
        child: Padding(
          padding: const EdgeInsets.all(16),
          child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            Text('Unlock to browse (read-only)', style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            TextField(
              controller: _c,
              obscureText: true,
              autofillHints: const [AutofillHints.password],
              decoration: const InputDecoration(labelText: 'Muninn password', border: OutlineInputBorder()),
              onSubmitted: (_) => _go(),
            ),
            if (_error != null) ...[
              const SizedBox(height: 8),
              Text(_error!, style: TextStyle(color: context.muninn.error)),
            ],
            const SizedBox(height: 8),
            FilledButton(
              onPressed: _busy ? null : _go,
              style: FilledButton.styleFrom(minimumSize: const Size.fromHeight(48)),
              child: _busy ? const SizedBox.square(dimension: 20, child: CircularProgressIndicator(strokeWidth: 2)) : const Text('Unlock'),
            ),
          ]),
        ),
      );
}

/// Shows [child] when unlocked, else the unlock card.
class Unlocked extends ConsumerWidget {
  const Unlocked({super.key, required this.child});
  final Widget child;
  @override
  Widget build(BuildContext context, WidgetRef ref) => ref.watch(unlockedProvider)
      ? child
      : ListView(padding: const EdgeInsets.all(16), children: const [UnlockCard()]);
}
