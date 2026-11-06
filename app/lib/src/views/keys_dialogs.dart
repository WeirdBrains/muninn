/// Keys write dialogs (`mnnn keys init|rotate|rekey|export|recover|shards`).
/// Recovery words and shards stay in a Rust reveal session; this UI fetches one
/// line at a time and dismisses the session when the sheet closes.
library;

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../state.dart';
import '../theme/muninn_theme.dart';

Future<void> showKeysInitDialog(BuildContext context, WidgetRef ref) =>
    _run(context, ref, child: const _InitDialog());

Future<void> showKeysRotateDialog(BuildContext context, WidgetRef ref) =>
    _run(context, ref, child: const _MasterDialog(kind: _Kind.rotate));

Future<void> showKeysRekeyDialog(BuildContext context, WidgetRef ref) =>
    _run(context, ref, child: const _MasterDialog(kind: _Kind.rekey));

Future<void> showKeysExportDialog(BuildContext context, WidgetRef ref) =>
    _run(context, ref, child: const _MasterDialog(kind: _Kind.export));

Future<void> showKeysRecoverWordsDialog(BuildContext context, WidgetRef ref) =>
    _run(context, ref, child: const _MasterDialog(kind: _Kind.recoverWords));

Future<void> showKeysRecoverShardsDialog(BuildContext context, WidgetRef ref) =>
    _run(context, ref, child: const _MasterDialog(kind: _Kind.recoverShards));

Future<void> showKeysReissueShardsDialog(BuildContext context, WidgetRef ref) =>
    _run(context, ref, child: const _MasterDialog(kind: _Kind.reissue));

Future<void> _run(
  BuildContext context,
  WidgetRef ref, {
  required Widget child,
}) => showDialog<void>(
  context: context,
  builder: (_) => child,
).whenComplete(() => ref.invalidate(keysStatusProvider));

enum _Kind { rotate, rekey, export, recoverWords, recoverShards, reissue }

class _InitDialog extends ConsumerStatefulWidget {
  const _InitDialog();
  @override
  ConsumerState<_InitDialog> createState() => _InitDialogState();
}

class _InitDialogState extends ConsumerState<_InitDialog> {
  final _a = TextEditingController();
  final _b = TextEditingController();
  String? _error;
  bool _busy = false;

  @override
  void dispose() {
    _a.dispose();
    _b.dispose();
    super.dispose();
  }

  Future<void> _go() async {
    setState(() {
      _error = null;
      _busy = true;
    });
    if (_a.text.isEmpty) {
      setState(() {
        _error = 'The password must not be empty.';
        _busy = false;
      });
      return;
    }
    if (_a.text != _b.text) {
      setState(() {
        _error = 'The two entries differ.';
        _busy = false;
      });
      return;
    }
    try {
      final h = await ref.read(apiProvider).keysInit(_a.text);
      if (!mounted) {
        await ref.read(apiProvider).keysRevealDismiss(h.id);
        return;
      }
      Navigator.pop(context);
      await showRevealSheet(
        context,
        ref,
        h,
        title: 'Recovery words and shards',
        showWords: true,
        showShards: true,
      );
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      title: Text(
        'Initialize keys',
        style: TextStyle(color: m.header, fontWeight: FontWeight.w800),
      ),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(
              'Creates the master key, service key and encrypted database. Recovery words and shards are shown once.',
              style: TextStyle(color: m.muted),
            ),
            const SizedBox(height: 12),
            TextField(
              key: const ValueKey('keys-init-password'),
              controller: _a,
              obscureText: true,
              decoration: const InputDecoration(
                labelText: 'New Muninn password',
              ),
            ),
            TextField(
              key: const ValueKey('keys-init-repeat'),
              controller: _b,
              obscureText: true,
              decoration: const InputDecoration(labelText: 'Repeat password'),
              onSubmitted: (_) => _go(),
            ),
            if (_error != null)
              Padding(
                padding: const EdgeInsets.only(top: 8),
                child: Text(_error!, style: TextStyle(color: m.error)),
              ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: _busy ? null : () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey('keys-init-go'),
          onPressed: _busy ? null : _go,
          child: _busy
              ? const SizedBox.square(
                  dimension: 18,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : const Text('Initialize'),
        ),
      ],
    );
  }
}

class _MasterDialog extends ConsumerStatefulWidget {
  const _MasterDialog({required this.kind});
  final _Kind kind;
  @override
  ConsumerState<_MasterDialog> createState() => _MasterDialogState();
}

class _MasterDialogState extends ConsumerState<_MasterDialog> {
  final _words = TextEditingController();
  final _shards = List.generate(5, (_) => TextEditingController());
  final _file = TextEditingController();
  final _filePw = TextEditingController();
  final _password = TextEditingController();
  final _password2 = TextEditingController();
  String _source = 'words';
  bool _masterFile = false;
  bool _busy = false;
  String? _error;
  String? _out;
  final _items = <String>{};
  bool _confirm = false;
  bool _printWords = false;

  @override
  void initState() {
    super.initState();
    if (widget.kind == _Kind.recoverShards) _source = 'shards';
    if (widget.kind == _Kind.export) {
      _items.addAll(['master', 'words', 'service', 'shards']);
    }
  }

  @override
  void dispose() {
    _words.dispose();
    for (final c in _shards) {
      c.dispose();
    }
    _file.dispose();
    _filePw.dispose();
    _password.dispose();
    _password2.dispose();
    super.dispose();
  }

  String? get _wordsArg => _source == 'words' ? _words.text : null;
  List<String> get _shardArg => _source == 'shards'
      ? _shards.map((c) => c.text).toList()
      : const <String>[];
  String? get _wordsFile =>
      _source == 'file' && !_masterFile ? _file.text : null;
  String? get _masterFileArg =>
      _source == 'file' && _masterFile ? _file.text : null;
  String? get _filePwArg => _filePw.text.isEmpty ? null : _filePw.text;

  Future<void> _pickFile() async {
    final r = await FilePicker.platform.pickFiles();
    if (r?.files.single.path != null) {
      setState(() => _file.text = r!.files.single.path!);
    }
  }

  Future<void> _pickOut() async {
    final d = await FilePicker.platform.getDirectoryPath();
    if (d != null) setState(() => _out = d);
  }

  Future<void> _go() async {
    setState(() {
      _error = null;
      _busy = true;
    });
    try {
      final api = ref.read(apiProvider);
      switch (widget.kind) {
        case _Kind.rotate:
          if (_password.text.isEmpty) throw 'The password must not be empty.';
          if (!_confirm) throw 'Confirm rotating the service key.';
          final v = await api.keysRotate(
            password: _password.text,
            words: _wordsArg,
            shards: _shardArg,
            wordsFile: _wordsFile,
            masterFile: _masterFileArg,
            filePassword: _filePwArg,
          );
          if (mounted) {
            Navigator.pop(context);
            ScaffoldMessenger.of(context).showSnackBar(
              SnackBar(content: Text('service key rotated to v$v')),
            );
          }
        case _Kind.rekey:
          if (!_confirm) throw 'Confirm re-encrypting the whole database.';
          await api.keysRekey(
            words: _wordsArg,
            shards: _shardArg,
            wordsFile: _wordsFile,
            masterFile: _masterFileArg,
            filePassword: _filePwArg,
          );
          ref.read(unlockedProvider.notifier).state = false;
          if (mounted) {
            Navigator.pop(context);
            ScaffoldMessenger.of(context).showSnackBar(
              const SnackBar(
                content: Text(
                  'database re-encrypted with a new data key; unlock again',
                ),
              ),
            );
          }
        case _Kind.export:
          if ((_out ?? '').isEmpty) throw 'Pick an output directory.';
          if (_password.text.isEmpty) {
            throw 'Export password must not be empty.';
          }
          final only = _items.toList();
          final combined =
              (_items.contains('master') || _items.contains('words')) &&
              _items.contains('shards');
          if (combined && !_confirm) throw 'Writing the master key/words and shards to one directory defeats the vault/shard split; confirm to continue.';
          final written = await api.keysExport(
            out: _out!,
            exportPassword: _password.text,
            only: only,
            confirmCombined: combined && _confirm,
            words: _wordsArg,
            shards: _shardArg,
            wordsFile: _wordsFile,
            masterFile: _masterFileArg,
            filePassword: _filePwArg,
          );
          if (mounted) {
            Navigator.pop(context);
            ScaffoldMessenger.of(context).showSnackBar(
              SnackBar(
                content: Text(
                  written.isEmpty ? 'exported' : 'wrote ${written.join(', ')}',
                ),
              ),
            );
          }
        case _Kind.recoverWords:
        case _Kind.recoverShards:
          if (_password.text.isEmpty) throw 'The password must not be empty.';
          if (_password.text != _password2.text) {
            throw 'The two entries differ.';
          }
          final h = await api.keysRecover(
            newPassword: _password.text,
            printWords: _printWords,
            words: _wordsArg,
            shards: _shardArg,
            wordsFile: _wordsFile,
            masterFile: _masterFileArg,
            filePassword: _filePwArg,
          );
          if (!mounted) {
            if (h != null) await api.keysRevealDismiss(h.id);
            return;
          }
          Navigator.pop(context);
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(
              content: Text(
                'access recovered; the service key is wrapped with the new password',
              ),
            ),
          );
          if (h != null) {
            await showRevealSheet(
              context,
              ref,
              h,
              title: 'Recovery words',
              showWords: true,
              showShards: false,
            );
          }
        case _Kind.reissue:
          if (!_confirm) throw 'Confirm re-issuing shards. The new set cannot be mixed with older shards.';
          final h = await api.keysReissueShards(
            words: _wordsArg,
            shards: _shardArg,
            wordsFile: _wordsFile,
            masterFile: _masterFileArg,
            filePassword: _filePwArg,
          );
          if (!mounted) {
            await api.keysRevealDismiss(h.id);
            return;
          }
          Navigator.pop(context);
          await showRevealSheet(
            context,
            ref,
            h,
            title: 'New shards',
            showWords: false,
            showShards: true,
          );
      }
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  String get _title => switch (widget.kind) {
    _Kind.rotate => 'Rotate service key',
    _Kind.rekey => 'Rekey database',
    _Kind.export => 'Export keys',
    _Kind.recoverWords => 'Recover from words',
    _Kind.recoverShards => 'Recover from shards',
    _Kind.reissue => 'Re-issue shards',
  };

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return AlertDialog(
      title: Text(
        _title,
        style: TextStyle(color: m.header, fontWeight: FontWeight.w800),
      ),
      content: SizedBox(
        width: 460,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (widget.kind == _Kind.rekey)
                Text(
                  'Rekeying rewrites the whole database. The app will lock first, the same idea as running it with the TUI closed.',
                  style: TextStyle(color: m.muted),
                ),
              if (widget.kind == _Kind.export)
                Text(
                  'Writes age-encrypted key files. Prefer a words file so the phrase never sits in the app; typing words here is the same as the TUI rotate form.',
                  style: TextStyle(color: m.muted),
                ),
              const SizedBox(height: 8),
              Wrap(
                spacing: 8,
                children: [
                  for (final s in ['words', 'shards', 'file'])
                    ChoiceChip(
                      label: Text(
                        s == 'file'
                            ? 'File'
                            : s == 'shards'
                            ? 'Shards'
                            : 'Words',
                      ),
                      selected: _source == s,
                      onSelected: (_) => setState(() => _source = s),
                    ),
                ],
              ),
              if (_source == 'words')
                TextField(
                  key: const ValueKey('keys-master-words'),
                  controller: _words,
                  obscureText: true,
                  decoration: const InputDecoration(
                    labelText: '24 recovery words',
                  ),
                ),
              if (_source == 'shards') ...[
                Text(
                  'Any 3 of 5 recover the master key.',
                  style: TextStyle(color: m.muted),
                ),
                for (var i = 0; i < _shards.length; i++)
                  TextField(
                    key: ValueKey('keys-master-shard-$i'),
                    controller: _shards[i],
                    obscureText: true,
                    decoration: InputDecoration(labelText: 'Shard ${i + 1}'),
                  ),
              ],
              if (_source == 'file') ...[
                SwitchListTile(
                  title: const Text('Master key file (instead of words file)'),
                  value: _masterFile,
                  onChanged: (v) => setState(() => _masterFile = v),
                ),
                TextField(
                  key: const ValueKey('keys-master-file'),
                  controller: _file,
                  decoration: const InputDecoration(labelText: 'Path'),
                  readOnly: true,
                  onTap: _pickFile,
                ),
                TextButton(
                  onPressed: _pickFile,
                  child: const Text('Pick file'),
                ),
                TextField(
                  controller: _filePw,
                  obscureText: true,
                  decoration: const InputDecoration(
                    labelText: 'File password (if encrypted)',
                  ),
                ),
              ],
              if (widget.kind == _Kind.rotate || widget.kind == _Kind.export)
                TextField(
                  key: const ValueKey('keys-action-password'),
                  controller: _password,
                  obscureText: true,
                  decoration: InputDecoration(
                    labelText: widget.kind == _Kind.export
                        ? 'Export password'
                        : 'Password (wraps the new service key)',
                  ),
                ),
              if (widget.kind == _Kind.recoverWords ||
                  widget.kind == _Kind.recoverShards) ...[
                TextField(
                  key: const ValueKey('keys-action-password'),
                  controller: _password,
                  obscureText: true,
                  decoration: const InputDecoration(
                    labelText: 'New Muninn password',
                  ),
                ),
                TextField(
                  controller: _password2,
                  obscureText: true,
                  decoration: const InputDecoration(
                    labelText: 'Repeat password',
                  ),
                ),
                SwitchListTile(
                  title: const Text('Show recovery words after recover'),
                  value: _printWords,
                  onChanged: (v) => setState(() => _printWords = v),
                ),
              ],
              if (widget.kind == _Kind.export) ...[
                Text(
                  _out ?? 'No directory picked',
                  style: TextStyle(color: m.muted),
                ),
                TextButton(
                  key: const ValueKey('keys-export-dir'),
                  onPressed: _pickOut,
                  child: const Text('Pick directory'),
                ),
                for (final it in ['master', 'words', 'service', 'shards'])
                  CheckboxListTile(
                    title: Text(it),
                    value: _items.contains(it),
                    dense: true,
                    onChanged: (v) => setState(
                      () => v == true ? _items.add(it) : _items.remove(it),
                    ),
                  ),
              ],
              if (widget.kind == _Kind.rotate ||
                  widget.kind == _Kind.rekey ||
                  widget.kind == _Kind.reissue ||
                  widget.kind == _Kind.export)
                CheckboxListTile(
                  key: const ValueKey('keys-confirm'),
                  value: _confirm,
                  onChanged: (v) => setState(() => _confirm = v ?? false),
                  title: Text(switch (widget.kind) {
                    _Kind.rotate => 'Rotate the service key',
                    _Kind.rekey =>
                      'Re-encrypt the whole database with a new data key',
                    _Kind.reissue => 'Re-issue shards (the new set cannot be mixed with older shards)',
                    _Kind.export =>
                      'Write master/words and shards to the same directory',
                    _ => '',
                  }),
                ),
              if (_error != null)
                Padding(
                  padding: const EdgeInsets.only(top: 8),
                  child: Text(_error!, style: TextStyle(color: m.error)),
                ),
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: _busy ? null : () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey('keys-action-go'),
          onPressed: _busy ? null : _go,
          child: _busy
              ? const SizedBox.square(
                  dimension: 18,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : const Text('Continue'),
        ),
      ],
    );
  }
}

/// One-time reveal: fetch each line from Rust, then dismiss the session.
Future<void> showRevealSheet(
  BuildContext context,
  WidgetRef ref,
  RevealHandle handle, {
  required String title,
  required bool showWords,
  required bool showShards,
}) async {
  final api = ref.read(apiProvider);
  try {
    await showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      showDragHandle: true,
      builder: (ctx) => _RevealBody(
        handle: handle,
        title: title,
        showWords: showWords,
        showShards: showShards,
      ),
    );
  } finally {
    await api.keysRevealDismiss(handle.id);
  }
}

class _RevealBody extends ConsumerStatefulWidget {
  const _RevealBody({
    required this.handle,
    required this.title,
    required this.showWords,
    required this.showShards,
  });
  final RevealHandle handle;
  final String title;
  final bool showWords, showShards;
  @override
  ConsumerState<_RevealBody> createState() => _RevealBodyState();
}

class _RevealBodyState extends ConsumerState<_RevealBody> {
  List<String> _words = const [];
  List<String> _shards = const [];
  bool _show = false;
  String? _error;
  bool _loading = true;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final api = ref.read(apiProvider);
      final words = <String>[];
      if (widget.showWords) {
        for (var i = 1; i <= widget.handle.wordCount; i++) {
          words.add(await api.keysRevealWord(widget.handle.id, i));
        }
      }
      final shards = <String>[];
      if (widget.showShards) {
        for (var i = 1; i <= widget.handle.shardCount; i++) {
          shards.add(await api.keysRevealShard(widget.handle.id, i));
        }
      }
      if (mounted) {
        setState(() {
          _words = words;
          _shards = shards;
          _show = widget.showWords; // init shows words immediately (CLI); shards-only starts hidden like the TUI
          if (!widget.showWords && widget.showShards) _show = false;
          _loading = false;
        });
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _error = '$e';
          _loading = false;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(16, 0, 16, 24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(
              widget.title,
              style: TextStyle(
                color: m.header,
                fontWeight: FontWeight.w800,
                fontSize: 18,
              ),
            ),
            const SizedBox(height: 8),
            Text(
              'Write these down. They are shown once and never stored. Use Export for encrypted copies.',
              style: TextStyle(color: m.muted),
            ),
            if (widget.handle.migratedPlainDb)
              Padding(
                padding: const EdgeInsets.only(top: 8),
                child: Text(
                  'The existing plaintext database was encrypted in place.',
                  style: TextStyle(color: m.warning),
                ),
              ),
            const SizedBox(height: 12),
            if (_loading) const LinearProgressIndicator(),
            if (_error != null) Text(_error!, style: TextStyle(color: m.error)),
            if (!_loading && !_show)
              Text(
                '${_words.length + _shards.length} secret line(s) hidden — Reveal to show them once',
                key: const ValueKey('keys-reveal-hidden'),
                style: TextStyle(color: m.muted),
              ),
            if (!_loading && _show) ...[
              if (_words.isNotEmpty) ...[
                Text(
                  'RECOVERY WORDS',
                  style: TextStyle(
                    color: m.header,
                    fontWeight: FontWeight.w700,
                  ),
                ),
                const SizedBox(height: 8),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    for (var i = 0; i < _words.length; i++)
                      SizedBox(
                        width: 140,
                        child: Text(
                          '${(i + 1).toString().padLeft(2)}. ${_words[i]}',
                          key: ValueKey('keys-word-$i'),
                          style: TextStyle(
                            color: m.warning,
                            fontFamily: 'monospace',
                          ),
                        ),
                      ),
                  ],
                ),
              ],
              if (_shards.isNotEmpty) ...[
                const SizedBox(height: 16),
                Text(
                  'DARK SHARDS (any 3 of 5)',
                  style: TextStyle(
                    color: m.header,
                    fontWeight: FontWeight.w700,
                  ),
                ),
                const SizedBox(height: 8),
                for (var i = 0; i < _shards.length; i++)
                  SelectableText(
                    _shards[i],
                    key: ValueKey('keys-shard-$i'),
                    style: TextStyle(
                      color: m.warning,
                      fontSize: 12,
                      fontFamily: 'monospace',
                    ),
                  ),
              ],
            ],
            const SizedBox(height: 16),
            Row(
              children: [
                if (!widget.showWords && widget.showShards)
                  TextButton(
                    key: const ValueKey('keys-reveal-toggle'),
                    onPressed: () => setState(() => _show = !_show),
                    child: Text(_show ? 'Hide' : 'Reveal'),
                  ),
                const Spacer(),
                FilledButton(
                  key: const ValueKey('keys-reveal-done'),
                  onPressed: () => Navigator.pop(context),
                  child: const Text('I have written these down'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
