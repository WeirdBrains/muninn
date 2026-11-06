/// Data ▸ Sources… (`mnnn sources`) and Data ▸ Symbols… (`mnnn symbols`).
/// TUI lists are read-only info modals; writes match the CLI. Source-data bytes
/// stay in Rust (`exportSourceData` writes them to a path).
library;

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../state.dart';
import '../theme/muninn_theme.dart';
import '../time_range.dart';

Future<void> showSourcesDialog(BuildContext context, WidgetRef ref) =>
    showDialog<void>(
      context: context,
      builder: (_) => const _SourcesDialog(),
    ).whenComplete(() => ref.invalidate(overviewProvider));

Future<void> showSymbolsDialog(BuildContext context, WidgetRef ref) =>
    showDialog<void>(
      context: context,
      builder: (_) => const _SymbolsDialog(),
    );

class _SourcesDialog extends ConsumerStatefulWidget {
  const _SourcesDialog();
  @override
  ConsumerState<_SourcesDialog> createState() => _SourcesDialogState();
}

class _SourcesDialogState extends ConsumerState<_SourcesDialog> {
  List<SourceInfo>? _items;
  String? _error;
  String? _selected;

  MuninnApi get _api => ref.read(apiProvider);

  @override
  void initState() {
    super.initState();
    _reload();
  }

  Future<void> _reload() async {
    try {
      final items = await _api.listSources();
      if (!mounted) return;
      setState(() {
        _items = items;
        _error = null;
        if (_selected != null && !items.any((s) => s.name == _selected)) {
          _selected = null;
        }
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = '$e');
    }
  }

  SourceInfo? get _sel {
    final name = _selected;
    if (name == null) return null;
    for (final s in _items ?? const <SourceInfo>[]) {
      if (s.name == name) return s;
    }
    return null;
  }

  Future<void> _add() async {
    final created = await showDialog<SourceInfo>(
      context: context,
      builder: (_) => const _AddSourceDialog(),
    );
    if (created == null) return;
    try {
      await _api.addSource(
        created.name,
        kind: created.kind,
        locator: created.locator,
      );
      await _reload();
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = '$e');
    }
  }

  Future<void> _rename() async {
    final s = _sel;
    if (s == null) return;
    final to = await showDialog<String>(
      context: context,
      builder: (_) => _RenameDialog(from: s.name),
    );
    if (to == null || to == s.name) return;
    try {
      await _api.renameSource(s.name, to);
      final key = ref.read(selectedDatasetProvider);
      if (key != null && key.startsWith('${s.name}:')) {
        ref.read(selectedDatasetProvider.notifier).state =
            '$to:${key.substring(s.name.length + 1)}';
      }
      _selected = to;
      await _reload();
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = '$e');
    }
  }

  Future<void> _remove() async {
    final s = _sel;
    if (s == null) return;
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Remove source?'),
        content: Text('remove source `${s.name}` and all of its bars?'),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            key: const ValueKey<String>('source-remove-confirm'),
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('Remove'),
          ),
        ],
      ),
    );
    if (ok != true) return;
    try {
      await _api.removeSource(s.name);
      final key = ref.read(selectedDatasetProvider);
      if (key != null && key.startsWith('${s.name}:')) {
        ref.read(selectedDatasetProvider.notifier).state = null;
      }
      _selected = null;
      await _reload();
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = '$e');
    }
  }

  Future<void> _viewData() async {
    final s = _sel;
    if (s == null) return;
    await showDialog<void>(
      context: context,
      builder: (_) => _SourceDataDialog(source: s),
    );
    await _reload();
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    final items = _items;
    return AlertDialog(
      key: const ValueKey<String>('sources-dialog'),
      title: Row(children: [
        const Expanded(child: Text('Sources')),
        IconButton(
          tooltip: 'Close',
          icon: const Text('[X]'),
          onPressed: () => Navigator.pop(context),
        ),
      ]),
      content: SizedBox(
        width: 520,
        height: 360,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            if (_error != null)
              Padding(
                padding: const EdgeInsets.only(bottom: 8),
                child: Text(_error!, style: TextStyle(color: m.error)),
              ),
            Expanded(
              child: items == null
                  ? const Center(child: CircularProgressIndicator())
                  : items.isEmpty
                  ? Text(
                      'no sources yet: File ▸ Import…',
                      style: TextStyle(color: m.muted),
                    )
                  : ListView.builder(
                      itemCount: items.length,
                      itemBuilder: (ctx, i) {
                        final s = items[i];
                        final loc = s.locator ?? '';
                        return ListTile(
                          key: ValueKey<String>('source-${s.name}'),
                          selected: s.name == _selected,
                          title: Text(s.name),
                          subtitle: Text(
                            '${s.kind}${loc.isEmpty ? '' : '  $loc'}  •  '
                            '${s.items} item(s), ${s.bytes} bytes',
                          ),
                          onTap: () => setState(() => _selected = s.name),
                        );
                      },
                    ),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          key: const ValueKey<String>('source-add'),
          onPressed: _add,
          child: const Text('Add…'),
        ),
        TextButton(
          key: const ValueKey<String>('source-rename'),
          onPressed: _sel == null ? null : _rename,
          child: const Text('Rename…'),
        ),
        TextButton(
          key: const ValueKey<String>('source-data'),
          onPressed: _sel == null ? null : _viewData,
          child: const Text('Stored data…'),
        ),
        TextButton(
          key: const ValueKey<String>('source-remove'),
          onPressed: _sel == null ? null : _remove,
          child: const Text('Remove'),
        ),
      ],
    );
  }
}

class _AddSourceDialog extends StatefulWidget {
  const _AddSourceDialog();
  @override
  State<_AddSourceDialog> createState() => _AddSourceDialogState();
}

class _AddSourceDialogState extends State<_AddSourceDialog> {
  final _name = TextEditingController();
  final _kind = TextEditingController(text: 'file');
  final _locator = TextEditingController();

  @override
  void dispose() {
    _name.dispose();
    _kind.dispose();
    _locator.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Add source'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          TextField(
            key: const ValueKey<String>('source-name'),
            controller: _name,
            autofocus: true,
            decoration: const InputDecoration(labelText: 'Name'),
          ),
          TextField(
            key: const ValueKey<String>('source-kind'),
            controller: _kind,
            decoration: const InputDecoration(
              labelText: 'Kind',
              hintText: 'file',
            ),
          ),
          TextField(
            key: const ValueKey<String>('source-locator'),
            controller: _locator,
            decoration: const InputDecoration(
              labelText: 'Locator (optional)',
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
          key: const ValueKey<String>('source-add-confirm'),
          onPressed: () {
            final name = _name.text.trim();
            if (name.isEmpty) return;
            final loc = _locator.text.trim();
            Navigator.pop(
              context,
              SourceInfo(
                id: '',
                name: name,
                kind: _kind.text.trim().isEmpty ? 'file' : _kind.text.trim(),
                locator: loc.isEmpty ? null : loc,
              ),
            );
          },
          child: const Text('Add'),
        ),
      ],
    );
  }
}

class _RenameDialog extends StatefulWidget {
  const _RenameDialog({required this.from});
  final String from;
  @override
  State<_RenameDialog> createState() => _RenameDialogState();
}

class _RenameDialogState extends State<_RenameDialog> {
  late final _to = TextEditingController(text: widget.from);

  @override
  void dispose() {
    _to.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: Text('Rename ${widget.from}'),
      content: TextField(
        key: const ValueKey<String>('source-rename-to'),
        controller: _to,
        autofocus: true,
        decoration: const InputDecoration(labelText: 'New name'),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey<String>('source-rename-confirm'),
          onPressed: () {
            final to = _to.text.trim();
            if (to.isEmpty) return;
            Navigator.pop(context, to);
          },
          child: const Text('Rename'),
        ),
      ],
    );
  }
}

class _SourceDataDialog extends ConsumerStatefulWidget {
  const _SourceDataDialog({required this.source});
  final SourceInfo source;
  @override
  ConsumerState<_SourceDataDialog> createState() => _SourceDataDialogState();
}

class _SourceDataDialogState extends ConsumerState<_SourceDataDialog> {
  List<SourceDataItem>? _items;
  String? _error;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final items = await ref.read(apiProvider).listSourceData(widget.source.name);
      if (!mounted) return;
      setState(() {
        _items = items;
        _error = null;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = '$e');
    }
  }

  Future<void> _save(SourceDataItem item) async {
    final picked = await FilePicker.platform.saveFile(fileName: item.name);
    if (picked == null || !mounted) return;
    try {
      await ref.read(apiProvider).exportSourceData(
        widget.source.name,
        item.id,
        picked,
      );
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = '$e');
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    final items = _items;
    return AlertDialog(
      key: const ValueKey<String>('source-data-dialog'),
      title: Text('Stored data — ${widget.source.name}'),
      content: SizedBox(
        width: 520,
        height: 320,
        child: items == null
            ? const Center(child: CircularProgressIndicator())
            : Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  if (_error != null)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 8),
                      child: Text(_error!, style: TextStyle(color: m.error)),
                    ),
                  Expanded(
                    child: items.isEmpty
                        ? Text(
                            'no stored items',
                            style: TextStyle(color: m.muted),
                          )
                        : ListView.builder(
                            itemCount: items.length,
                            itemBuilder: (ctx, i) {
                              final it = items[i];
                              final id = it.id.length > 8
                                  ? it.id.substring(0, 8)
                                  : it.id;
                              return ListTile(
                                key: ValueKey<String>('source-data-${it.id}'),
                                title: Text(it.name),
                                subtitle: Text(
                                  '$id  ${it.receivedAt == 0 ? '' : '${fmtUtc(it.receivedAt)} UTC  '}'
                                  '${it.format}  ${it.size} bytes'
                                  '${it.locator.isEmpty ? '' : '  ${it.locator}'}',
                                ),
                                trailing: TextButton(
                                  onPressed: () => _save(it),
                                  child: const Text('Save…'),
                                ),
                              );
                            },
                          ),
                  ),
                ],
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

class _SymbolsDialog extends ConsumerStatefulWidget {
  const _SymbolsDialog();
  @override
  ConsumerState<_SymbolsDialog> createState() => _SymbolsDialogState();
}

class _SymbolsDialogState extends ConsumerState<_SymbolsDialog> {
  List<SymbolInfo>? _symbols;
  List<AliasInfo>? _aliases;
  String? _error;

  @override
  void initState() {
    super.initState();
    _reload();
  }

  Future<void> _reload() async {
    final api = ref.read(apiProvider);
    try {
      final symbols = await api.listSymbols();
      final aliases = await api.listAliases();
      if (!mounted) return;
      setState(() {
        _symbols = symbols;
        _aliases = aliases;
        _error = null;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = '$e');
    }
  }

  Future<void> _add() async {
    final result = await showDialog<({String symbol, List<String> aliases})>(
      context: context,
      builder: (_) => const _AddSymbolDialog(),
    );
    if (result == null) return;
    try {
      await ref.read(apiProvider).addSymbol(
        result.symbol,
        aliases: result.aliases,
      );
      await _reload();
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = '$e');
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    final symbols = _symbols;
    final aliases = _aliases ?? const <AliasInfo>[];
    return AlertDialog(
      key: const ValueKey<String>('symbols-dialog'),
      title: Row(children: [
        const Expanded(child: Text('Symbols')),
        IconButton(
          tooltip: 'Close',
          icon: const Text('[X]'),
          onPressed: () => Navigator.pop(context),
        ),
      ]),
      content: SizedBox(
        width: 420,
        height: 360,
        child: symbols == null
            ? const Center(child: CircularProgressIndicator())
            : Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  if (_error != null)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 8),
                      child: Text(_error!, style: TextStyle(color: m.error)),
                    ),
                  Expanded(
                    child: ListView(
                      children: [
                        if (symbols.isEmpty)
                          Text(
                            'no symbols yet',
                            style: TextStyle(color: m.muted),
                          ),
                        for (final s in symbols)
                          ListTile(
                            key: ValueKey<String>('symbol-${s.symbol}'),
                            title: Text(s.symbol),
                            dense: true,
                          ),
                        if (aliases.isNotEmpty) ...[
                          const Divider(),
                          for (final a in aliases)
                            ListTile(
                              key: ValueKey<String>('alias-${a.alias}'),
                              title: Text('alias ${a.alias} -> ${a.canonical}'),
                              dense: true,
                            ),
                        ],
                      ],
                    ),
                  ),
                ],
              ),
      ),
      actions: [
        TextButton(
          key: const ValueKey<String>('symbol-add'),
          onPressed: _add,
          child: const Text('Add…'),
        ),
      ],
    );
  }
}

class _AddSymbolDialog extends StatefulWidget {
  const _AddSymbolDialog();
  @override
  State<_AddSymbolDialog> createState() => _AddSymbolDialogState();
}

class _AddSymbolDialogState extends State<_AddSymbolDialog> {
  final _symbol = TextEditingController();
  final _aliases = TextEditingController();

  @override
  void dispose() {
    _symbol.dispose();
    _aliases.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Add symbol'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          TextField(
            key: const ValueKey<String>('symbol-name'),
            controller: _symbol,
            autofocus: true,
            decoration: const InputDecoration(
              labelText: 'Symbol',
              hintText: 'BTC/USDT',
            ),
          ),
          TextField(
            key: const ValueKey<String>('symbol-aliases'),
            controller: _aliases,
            decoration: const InputDecoration(
              labelText: 'Aliases for the base (optional)',
              hintText: 'XBT',
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
          key: const ValueKey<String>('symbol-add-confirm'),
          onPressed: () {
            final symbol = _symbol.text.trim();
            if (symbol.isEmpty) return;
            final aliases = _aliases.text
                .split(RegExp(r'[\s,]+'))
                .map((s) => s.trim())
                .where((s) => s.isNotEmpty)
                .toList();
            Navigator.pop(context, (symbol: symbol, aliases: aliases));
          },
          child: const Text('Add'),
        ),
      ],
    );
  }
}
