/// In-app help: the same embedded docs as `mnnn help` and the TUI (`?` / F1).
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../api.dart';
import '../shell.dart' show phoneBreakpoint;
import '../state.dart';
import '../theme/muninn_theme.dart';

/// Topic opened by `?` / F1 for the current section (TUI context-sensitive help).
String helpTopicForSection(Section s, {bool asChart = true}) => switch (s) {
  Section.home => 'flutter-app',
  Section.data => asChart ? 'charts' : 'sidebar',
  Section.projects => 'projects',
  Section.net => 'network',
  Section.orgs => 'organizations',
  Section.keys => 'keys',
  Section.help => 'help',
};

void openHelp(WidgetRef ref, {String? topic, bool search = false}) {
  if (topic != null) {
    ref.read(helpTopicIdProvider.notifier).state = topic;
  } else if (ref.read(sectionProvider) != Section.help) {
    ref.read(helpTopicIdProvider.notifier).state = helpTopicForSection(
      ref.read(sectionProvider),
      asChart: ref.read(dataAsChartProvider),
    );
  }
  ref.read(helpSearchOpenProvider.notifier).state = search;
  if (search) ref.read(helpSearchQueryProvider.notifier).state = '';
  ref.read(sectionProvider.notifier).state = Section.help;
}

class HelpView extends ConsumerStatefulWidget {
  const HelpView({super.key});
  @override
  ConsumerState<HelpView> createState() => _HelpViewState();
}

class _HelpViewState extends ConsumerState<HelpView> {
  final _search = TextEditingController();
  final _searchFocus = FocusNode();
  final _history = <String>[];

  @override
  void dispose() {
    _search.dispose();
    _searchFocus.dispose();
    super.dispose();
  }

  Future<void> _open(String id, {bool record = true}) async {
    final cur = ref.read(helpTopicIdProvider);
    if (record && cur.isNotEmpty && cur != id) _history.add(cur);
    ref.read(helpTopicIdProvider.notifier).state = id;
    ref.read(helpSearchQueryProvider.notifier).state = '';
    ref.read(helpSearchOpenProvider.notifier).state = false;
    _search.clear();
    setState(() {});
  }

  void _back() {
    if (_history.isEmpty) return;
    final prev = _history.removeLast();
    ref.read(helpTopicIdProvider.notifier).state = prev;
    setState(() {});
  }

  Future<void> _follow(String from, String href) async {
    final target = await ref.read(apiProvider).helpResolveHref(from, href);
    if (target != null) {
      await _open(target);
      return;
    }
    if (!mounted) return;
    final m = context.muninn;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Text(href, style: TextStyle(color: m.fg)),
        backgroundColor: m.bg,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final m = context.muninn;
    final id = ref.watch(helpTopicIdProvider);
    final wantSearch = ref.watch(helpSearchOpenProvider);
    if (wantSearch && !_searchFocus.hasFocus) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (_searchFocus.canRequestFocus) _searchFocus.requestFocus();
        ref.read(helpSearchOpenProvider.notifier).state = false;
      });
    }
    final q = ref.watch(helpSearchQueryProvider);
    final phone = MediaQuery.sizeOf(context).width < phoneBreakpoint;
    final topic = ref.watch(_topicProvider(id));
    final results = q.trim().isEmpty ? null : ref.watch(_searchProvider(q));

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (phone)
          Padding(
            padding: const EdgeInsets.fromLTRB(8, 8, 8, 0),
            child: Row(
              children: [
                Expanded(
                  child: OutlinedButton.icon(
                    key: const ValueKey('help-goto-orgs'),
                    onPressed: () =>
                        ref.read(sectionProvider.notifier).state = Section.orgs,
                    icon: const Icon(Icons.groups_outlined),
                    label: const Text('Organizations'),
                  ),
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: OutlinedButton.icon(
                    key: const ValueKey('help-goto-keys'),
                    onPressed: () =>
                        ref.read(sectionProvider.notifier).state = Section.keys,
                    icon: const Icon(Icons.key_outlined),
                    label: const Text('Keys'),
                  ),
                ),
              ],
            ),
          ),
        Padding(
          padding: const EdgeInsets.fromLTRB(12, 8, 12, 4),
          child: Row(
            children: [
              IconButton(
                tooltip: 'Back',
                key: const ValueKey('help-back'),
                onPressed: _history.isEmpty ? null : _back,
                icon: const Icon(Icons.arrow_back),
              ),
              IconButton(
                tooltip: 'Index',
                key: const ValueKey('help-index'),
                onPressed: () => _open('README'),
                icon: const Icon(Icons.home_outlined),
              ),
              Expanded(
                child: TextField(
                  key: const ValueKey('help-search'),
                  controller: _search,
                  focusNode: _searchFocus,
                  decoration: const InputDecoration(
                    hintText: 'Search help',
                    prefixIcon: Icon(Icons.search),
                    isDense: true,
                  ),
                  onChanged: (v) =>
                      ref.read(helpSearchQueryProvider.notifier).state = v,
                ),
              ),
            ],
          ),
        ),
        Expanded(
          child: results != null
              ? results.when(
                  loading: () =>
                      const Center(child: CircularProgressIndicator()),
                  error: (e, _) => Padding(
                    padding: const EdgeInsets.all(16),
                    child: Text('$e', style: TextStyle(color: m.error)),
                  ),
                  data: (list) => list.isEmpty
                      ? Padding(
                          padding: const EdgeInsets.all(16),
                          child: Text(
                            'No topics match that search',
                            style: TextStyle(color: m.muted),
                          ),
                        )
                      : ListView(
                          children: [
                            for (final t in list)
                              ListTile(
                                key: ValueKey('help-hit-${t.id}'),
                                minTileHeight: 48,
                                title: Text(t.title),
                                subtitle: Text(t.id),
                                onTap: () => _open(t.id),
                              ),
                          ],
                        ),
                )
              : topic.when(
                  loading: () =>
                      const Center(child: CircularProgressIndicator()),
                  error: (e, _) => Padding(
                    padding: const EdgeInsets.all(16),
                    child: Text('$e', style: TextStyle(color: m.error)),
                  ),
                  data: (t) => ListView(
                    padding: const EdgeInsets.fromLTRB(16, 8, 16, 24),
                    children: [
                      Text(
                        t.title,
                        key: const ValueKey('help-title'),
                        style: TextStyle(
                          color: m.header,
                          fontWeight: FontWeight.w800,
                          fontSize: 22,
                        ),
                      ),
                      const SizedBox(height: 12),
                      ...helpMarkdown(t.body, m, (href) => _follow(t.id, href)),
                    ],
                  ),
                ),
        ),
      ],
    );
  }
}

final _topicProvider = FutureProvider.family<HelpTopic, String>(
  (ref, id) => ref.watch(apiProvider).helpTopic(id),
);
final _searchProvider = FutureProvider.family<List<HelpTopicInfo>, String>(
  (ref, q) => ref.watch(apiProvider).helpSearch(q),
);

/// A compact Markdown renderer for help topics (headings, lists, tables, code, links).
List<Widget> helpMarkdown(
  String body,
  MuninnColors m,
  void Function(String href) onLink,
) {
  final out = <Widget>[];
  final lines = body.split('\n');
  var i = 0;
  final para = <String>[];

  void flush() {
    if (para.isEmpty) return;
    out.add(
      Padding(
        padding: const EdgeInsets.only(bottom: 10),
        child: SelectableText.rich(
          TextSpan(children: _inline(para.join(' '), m, onLink)),
          style: TextStyle(color: m.fg, height: 1.35),
        ),
      ),
    );
    para.clear();
  }

  while (i < lines.length) {
    final line = lines[i];
    if (line.trimLeft().startsWith('```')) {
      flush();
      final code = <String>[];
      i++;
      while (i < lines.length && !lines[i].trimLeft().startsWith('```')) {
        code.add(lines[i]);
        i++;
      }
      out.add(
        Container(
          width: double.infinity,
          margin: const EdgeInsets.only(bottom: 12),
          padding: const EdgeInsets.all(10),
          color: m.bg,
          child: SelectableText(
            code.join('\n'),
            style: TextStyle(
              color: m.accent,
              fontFamily: 'monospace',
              fontSize: 13,
            ),
          ),
        ),
      );
      i++;
      continue;
    }
    if (line.startsWith('#')) {
      flush();
      final n = line.takeWhile((c) => c == '#');
      final text = line.substring(n).trim();
      out.add(
        Padding(
          padding: EdgeInsets.only(top: n == 1 ? 4 : 12, bottom: 8),
          child: SelectableText(
            text,
            style: TextStyle(
              color: m.header,
              fontWeight: FontWeight.w800,
              fontSize: n == 1
                  ? 22
                  : n == 2
                  ? 18
                  : 16,
            ),
          ),
        ),
      );
      i++;
      continue;
    }
    if (line.startsWith('|') &&
        i + 1 < lines.length &&
        lines[i + 1].contains('---')) {
      flush();
      final rows = <List<String>>[];
      while (i < lines.length && lines[i].startsWith('|')) {
        final cells = lines[i].split('|');
        final trimmed = [
          for (var c = 1; c < cells.length - 1; c++) cells[c].trim(),
        ];
        if (!trimmed.every((c) => RegExp(r'^:?-+:?$').hasMatch(c))) {
          rows.add(trimmed);
        }
        i++;
      }
      if (rows.isNotEmpty) {
        out.add(
          Padding(
            padding: const EdgeInsets.only(bottom: 12),
            child: Table(
              border: TableBorder.all(color: m.border),
              children: [
                for (var r = 0; r < rows.length; r++)
                  TableRow(
                    children: [
                      for (final c in rows[r])
                        Padding(
                          padding: const EdgeInsets.all(6),
                          child: SelectableText.rich(
                            TextSpan(children: _inline(c, m, onLink)),
                            style: TextStyle(
                              color: r == 0 ? m.header : m.fg,
                              fontWeight: r == 0
                                  ? FontWeight.w700
                                  : FontWeight.w400,
                              fontSize: 13,
                            ),
                          ),
                        ),
                    ],
                  ),
              ],
            ),
          ),
        );
      }
      continue;
    }
    if (line.trim() == '---' || line.trim() == '***') {
      flush();
      out.add(Divider(color: m.border));
      i++;
      continue;
    }
    if (line.startsWith('> ')) {
      flush();
      final q = <String>[];
      while (i < lines.length && lines[i].startsWith('> ')) {
        q.add(lines[i].substring(2));
        i++;
      }
      out.add(
        Container(
          width: double.infinity,
          margin: const EdgeInsets.only(bottom: 12),
          padding: const EdgeInsets.fromLTRB(12, 8, 8, 8),
          decoration: BoxDecoration(
            border: Border(left: BorderSide(color: m.accent, width: 3)),
          ),
          child: SelectableText.rich(
            TextSpan(children: _inline(q.join(' '), m, onLink)),
            style: TextStyle(color: m.muted),
          ),
        ),
      );
      continue;
    }
    final bullet =
        line.startsWith('- ') ||
        line.startsWith('* ') ||
        RegExp(r'^\d+\. ').hasMatch(line);
    if (bullet) {
      flush();
      while (i < lines.length &&
          (lines[i].startsWith('- ') ||
              lines[i].startsWith('* ') ||
              RegExp(r'^\d+\. ').hasMatch(lines[i]))) {
        final t = lines[i].replaceFirst(RegExp(r'^(- |\* |\d+\. )'), '');
        out.add(
          Padding(
            padding: const EdgeInsets.only(left: 8, bottom: 4),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('•  ', style: TextStyle(color: m.accent)),
                Expanded(
                  child: SelectableText.rich(
                    TextSpan(children: _inline(t, m, onLink)),
                    style: TextStyle(color: m.fg),
                  ),
                ),
              ],
            ),
          ),
        );
        i++;
      }
      out.add(const SizedBox(height: 6));
      continue;
    }
    if (line.trim().isEmpty) {
      flush();
      i++;
      continue;
    }
    para.add(line.trim());
    i++;
  }
  flush();
  return out;
}

extension on String {
  int takeWhile(bool Function(String c) test) {
    var n = 0;
    for (var i = 0; i < length; i++) {
      if (!test(this[i])) break;
      n++;
    }
    return n;
  }
}

List<InlineSpan> _inline(
  String text,
  MuninnColors m,
  void Function(String href) onLink,
) {
  final spans = <InlineSpan>[];
  final re = RegExp(
    r'(\[([^\]]+)\]\(([^)]+)\)|`([^`]+)`|\*\*([^*]+)\*\*|\*([^*]+)\*)',
  );
  var start = 0;
  for (final match in re.allMatches(text)) {
    if (match.start > start) {
      spans.add(TextSpan(text: text.substring(start, match.start)));
    }
    if (match[1] != null && match[1]!.startsWith('[')) {
      final label = match[2]!;
      final href = match[3]!;
      spans.add(
        WidgetSpan(
          alignment: PlaceholderAlignment.baseline,
          baseline: TextBaseline.alphabetic,
          child: GestureDetector(
            onTap: () => onLink(href),
            child: Text(
              label,
              style: TextStyle(
                color: m.accent,
                decoration: TextDecoration.underline,
              ),
            ),
          ),
        ),
      );
    } else if (match[4] != null) {
      spans.add(
        TextSpan(
          text: match[4],
          style: TextStyle(color: m.accent, fontFamily: 'monospace'),
        ),
      );
    } else if (match[5] != null) {
      spans.add(
        TextSpan(
          text: match[5],
          style: const TextStyle(fontWeight: FontWeight.w700),
        ),
      );
    } else if (match[6] != null) {
      spans.add(
        TextSpan(
          text: match[6],
          style: const TextStyle(fontStyle: FontStyle.italic),
        ),
      );
    }
    start = match.end;
  }
  if (start < text.length) spans.add(TextSpan(text: text.substring(start)));
  return spans;
}
