// Publishes docs/*.md (all but docs/historical) as pages under build/jaspr/docs/<slug>/,
// with a sidebar grouped like docs/README.md. Run after `jaspr build`:
//   dart run tool/docs.dart [out]
// Links between docs become site pages; links elsewhere in the repo go to GitHub.
import 'dart:convert';
import 'dart:io';

import 'package:markdown/markdown.dart' as md;

import 'package:muninn_site/logo.dart';

const repo = 'https://github.com/WeirdBrains/muninn';

/// A doc: its path under docs/ (`services/redis.md`), slug and title.
class Doc {
  Doc(this.path, this.source)
    : slug = path == 'README.md' ? '' : path.substring(0, path.length - 3),
      title = _title(source, path);
  final String path;
  final String source;
  final String slug;
  final String title;
  String get url => slug.isEmpty ? '/docs/' : '/docs/$slug/';

  static String _title(String src, String path) {
    for (final l in LineSplitter.split(src)) {
      if (l.startsWith('# ')) return l.substring(2).trim();
    }
    return path;
  }
}

/// Every user-facing doc under [root] (docs/), sorted by path.
List<Doc> collect(Directory root) {
  final docs = <Doc>[];
  for (final f in root.listSync(recursive: true).whereType<File>()) {
    final rel = f.path.substring(root.path.length + 1).replaceAll('\\', '/');
    if (!rel.endsWith('.md') || rel.startsWith('historical/')) continue;
    docs.add(Doc(rel, f.readAsStringSync()));
  }
  docs.sort((a, b) => a.path.compareTo(b.path));
  return docs;
}

/// Normalise `a/../b/./c` to `b/c`; null if it climbs out of the repo.
String? normalise(String p) {
  final out = <String>[];
  for (final s in p.split('/')) {
    if (s == '' || s == '.') continue;
    if (s == '..') {
      if (out.isEmpty) return null;
      out.removeLast();
    } else {
      out.add(s);
    }
  }
  return out.join('/');
}

/// Where a link in doc [from] should point on the site. Paths are repo-relative
/// (`docs/keys.md`), so `../site/web/screenshots/x.png` becomes `/screenshots/x.png`.
String rewrite(String from, String href, Set<String> pages) {
  if (href.startsWith('#') ||
      href.contains('://') ||
      href.startsWith('mailto:')) {
    return href;
  }
  final hash = href.indexOf('#');
  final path = hash < 0 ? href : href.substring(0, hash);
  final frag = hash < 0 ? '' : href.substring(hash);
  final dir = from.contains('/')
      ? from.substring(0, from.lastIndexOf('/'))
      : '';
  final full = normalise('docs/${dir.isEmpty ? '' : '$dir/'}$path');
  if (full == null) return href;
  if (full.startsWith('docs/') && pages.contains(full.substring(5))) {
    final rel = full.substring(5);
    return (rel == 'README.md'
            ? '/docs/'
            : '/docs/${rel.substring(0, rel.length - 3)}/') +
        frag;
  }
  if (full.startsWith('site/web/')) return '/${full.substring(9)}$frag';
  final kind = full.endsWith('/') || !full.split('/').last.contains('.')
      ? 'tree'
      : 'blob';
  return '$repo/$kind/main/$full$frag';
}

/// Sidebar groups from docs/README.md: each `## ` heading and the docs linked under it.
List<(String, List<Doc>)> groups(List<Doc> docs) {
  final byPath = {for (final d in docs) d.path: d};
  final readme = byPath['README.md'];
  final out = <(String, List<Doc>)>[];
  final seen = <String>{'README.md'};
  if (readme != null) {
    String? group;
    var items = <Doc>[];
    final link = RegExp(r'\]\(([^)#\s]+\.md)(#[^)]*)?\)');
    for (final l in LineSplitter.split(readme.source)) {
      if (l.startsWith('## ')) {
        if (group != null && items.isNotEmpty) out.add((group, items));
        group = l.substring(3).trim();
        items = [];
      } else if (group != null) {
        for (final m in link.allMatches(l)) {
          final p = normalise(m.group(1)!);
          final d = p == null ? null : byPath[p];
          if (d != null && seen.add(d.path)) items.add(d);
        }
      }
    }
    if (group != null && items.isNotEmpty) out.add((group, items));
  }
  final rest = docs.where((d) => !seen.contains(d.path)).toList();
  if (rest.isNotEmpty) out.add(('More', rest));
  return out;
}

String esc(String s) => const HtmlEscape().convert(s);

/// The page body: Markdown to HTML with links rewritten.
String body(Doc d, Set<String> pages) {
  final doc = md.Document(
    extensionSet: md.ExtensionSet.gitHubWeb,
    encodeHtml: true,
  );
  final nodes = doc.parse(d.source);
  void walk(List<md.Node>? ns) {
    for (final n in ns ?? const <md.Node>[]) {
      if (n is md.Element) {
        for (final attr in ['href', 'src']) {
          final v = n.attributes[attr];
          if (v != null) n.attributes[attr] = rewrite(d.path, v, pages);
        }
        walk(n.children);
      }
    }
  }

  walk(nodes);
  return md.renderToHtml(nodes);
}

String page(Doc d, List<(String, List<Doc>)> nav, Set<String> pages) {
  final side = StringBuffer()
    ..write(
      '<a class="doc-home${d.slug.isEmpty ? ' current' : ''}" href="/docs/">Documentation</a>',
    );
  for (final (g, items) in nav) {
    side.write('<h4>${esc(g)}</h4><ul>');
    for (final i in items) {
      final cur = i.path == d.path
          ? ' class="current" aria-current="page"'
          : '';
      side.write('<li><a$cur href="${i.url}">${esc(i.title)}</a></li>');
    }
    side.write('</ul>');
  }
  return '''<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="dark light">
<title>${esc(d.title)} · Muninn docs</title>
<link rel="stylesheet" href="/styles.css">
<link rel="icon" href="/favicon.svg" type="image/svg+xml">
<link rel="icon" href="/favicon-32.png" type="image/png" sizes="32x32">
<link rel="icon" href="/favicon-16.png" type="image/png" sizes="16x16">
<link rel="apple-touch-icon" href="/apple-touch-icon.png">
</head>
<body class="docs-page">
<header class="docs-top"><a class="brand" href="/"><span class="logo" aria-hidden="true">$ravenMark</span>MUNINN</a><nav><a href="/docs/">Docs</a><a href="/#install">Install</a><a href="$repo/releases">Releases</a><a href="$repo">Source</a></nav></header>
<div class="docs-layout">
<nav class="docs-nav" aria-label="Documentation">$side</nav>
<main class="doc">
${body(d, pages)}
<p class="edit"><a href="$repo/edit/main/docs/${d.path}">Edit this page on GitHub</a></p>
</main>
</div>
</body>
</html>
''';
}

void main(List<String> args) {
  final out = Directory(args.isEmpty ? 'build/jaspr' : args.first);
  final root = Directory('../docs');
  final docs = collect(root);
  final pages = {for (final d in docs) d.path};
  final nav = groups(docs);
  final target = Directory('${out.path}/docs');
  if (target.existsSync()) target.deleteSync(recursive: true);
  for (final d in docs) {
    final f = File(
      '${target.path}/${d.slug.isEmpty ? '' : '${d.slug}/'}index.html',
    );
    f.parent.createSync(recursive: true);
    f.writeAsStringSync(page(d, nav, pages));
  }
  stdout.writeln('docs: ${docs.length} pages in ${target.path}');
}
