import 'package:jaspr/jaspr.dart';
import 'package:jaspr/dom.dart';

import 'banner.dart';
import 'logo.dart';
import 'shots.dart';

const repo = 'https://github.com/WeirdBrains/muninn';
const releases = '$repo/releases';
const docs = '/docs/';
const shotWidth = 1464;
const shotHeight = 890;
const installDoc = '/docs/install/';
const homepage = 'https://muninn.weirdbrains.com';

/// The released version (scripts/release.sh bumps it; tests/site.rs checks it).
const version = '2.20.1';

/// The whole site: one page, no client-side code.
class App extends StatelessComponent {
  const App({super.key});

  @override
  Component build(BuildContext context) {
    return div(classes: 'page', [
      header(classes: 'hero', [
        span(
          classes: 'logo hero-logo',
          attributes: {'aria-hidden': 'true'},
          [RawText(ravenTile)],
        ),
        pre(
          classes: 'banner',
          attributes: {'aria-label': 'Muninn'},
          [Component.text(banner.join('\n'))],
        ),
        figure(classes: 'hero-shot', [
          img(
            src: heroShot.$1,
            alt: heroShot.$2,
            width: shotWidth,
            height: shotHeight,
          ),
          figcaption([Component.text(heroShot.$2)]),
        ]),
        p(classes: 'tagline', [
          Component.text(
            'A local, encrypted workbench for market data: ingest, search, journal, model and share, in a terminal UI and a CLI.',
          ),
        ]),
        nav(classes: 'links', [
          a(href: releases, classes: 'button primary', [
            Component.text('Releases'),
          ]),
          a(href: '#install', classes: 'button', [Component.text('Install')]),
          a(href: '#screenshots', classes: 'button', [
            Component.text('Screenshots'),
          ]),
          a(href: docs, classes: 'button', [Component.text('Docs')]),
          a(href: repo, classes: 'button', [Component.text('Source')]),
        ]),
      ]),
      section(classes: 'features', [
        for (final (title, body) in features)
          div(classes: 'card', [
            h3([Component.text(title)]),
            p([Component.text(body)]),
          ]),
      ]),
      section(id: 'screenshots', classes: 'screenshots', [
        h2([Component.text('Screenshots')]),
        p(classes: 'muted', [
          Component.text(
            'Every view in the TUI, rendered from the app itself by scripts/screenshots.sh.',
          ),
        ]),
        div(classes: 'shots', [
          for (final (file, caption) in shots)
            figure([
              a(href: file, [
                img(
                  src: file,
                  alt: caption,
                  width: shotWidth,
                  height: shotHeight,
                  loading: MediaLoading.lazy,
                ),
              ]),
              figcaption([Component.text(caption)]),
            ]),
        ]),
      ]),
      section(id: 'install', classes: 'install', [
        h2([Component.text('Install')]),
        p([
          Component.text('Every package ships a prebuilt '),
          code([Component.text('mnnn')]),
          Component.text(
            ' binary; no Rust toolchain is needed. Downloads and SHA256SUMS are on the ',
          ),
          a(href: releases, [Component.text('releases page')]),
          Component.text(
            '. Full steps, uninstalling and building from source: ',
          ),
          a(href: installDoc, [Component.text('the install guide')]),
          Component.text('.'),
        ]),
        for (final (label, cmd) in installs)
          div(classes: 'step', [
            h4([Component.text(label)]),
            pre([
              code([Component.text(cmd)]),
            ]),
          ]),
        p(classes: 'muted', [
          Component.text(
            'Your data stays in ~/.muninn (or MUNINN_HOME); installing or uninstalling never touches it.',
          ),
        ]),
      ]),
      section(classes: 'docs', [
        h2([Component.text('Docs')]),
        ul([
          for (final (name, file) in docPages)
            li([
              a(href: '/docs/${file.substring(0, file.length - 3)}/', [
                Component.text(name),
              ]),
            ]),
        ]),
        p(classes: 'muted', [
          Component.text('The same pages are in the app: '),
          code([Component.text('mnnn help')]),
          Component.text(' or ? in the TUI, and man pages for every command.'),
        ]),
      ]),
      footer(classes: 'about', [
        span(
          classes: 'logo about-logo',
          attributes: {'aria-hidden': 'true'},
          [RawText(ravenMark)],
        ),
        p([
          Component.text('Muninn $version · published by WeirdBrains · '),
          a(href: homepage, [Component.text('muninn.weirdbrains.com')]),
        ]),
        p([
          Component.text('Muninn · '),
          a(href: releases, [Component.text('releases')]),
          Component.text(' · '),
          a(href: '$repo/blob/main/CHANGELOG.md', [
            Component.text('changelog'),
          ]),
          Component.text(' · '),
          a(href: docs, [Component.text('docs')]),
          Component.text(' · '),
          a(href: '$repo/blob/main/LICENSE', [Component.text('BSD 3-Clause')]),
        ]),
      ]),
    ]);
  }
}

const features = [
  (
    'Encrypted and local',
    'A SQLCipher database under an envelope key scheme. Nothing leaves your machine unless you share it.',
  ),
  (
    'Ingest anything',
    'CSV, JSON, HTTP APIs, Redis, SQLite, Postgres and standard input, as structured data.',
  ),
  (
    'Live searches, journals, reports',
    'Saved questions that rerun on new data, notes beside the charts, reports exported to PDF and Docx.',
  ),
  (
    'Models and code spaces',
    'Design model tables, then run math, Polars, Rust or Python over data piped through them.',
  ),
  (
    'Pipelines',
    'Graphs of sources, code, views and filters ending in model tables, watched live in tiles.',
  ),
  (
    'Rules and schedules',
    'If this happens, do that, or every 6 hours: exports, entries, records, certificate checks.',
  ),
  (
    'Organizations',
    'Members, roles and a hosted site with your own domains and Let\'s Encrypt certificates.',
  ),
  (
    'Peer to peer',
    'Your nodes sync end to end encrypted; share and review with connections, no server needed.',
  ),
];

const installs = [
  (
    'Windows (setup.exe)',
    'Download muninn-2.20.1-x86_64-windows-setup.exe from the release and run it.\nSmartScreen: More info, then Run anyway. Firewall prompt: Allow for peer-to-peer.\nOr: winget install WeirdBrains.Muninn',
  ),
  (
    'macOS (dmg)',
    'Download muninn-2.20.1-macos.dmg, drag Muninn to Applications,\nthen right-click Muninn and choose Open the first time.',
  ),
  ('Homebrew (macOS, Linux)', 'brew install weirdbrains/tap/muninn'),
  ('Debian / Ubuntu', 'sudo apt install ./muninn_2.20.1-1_amd64.deb'),
  ('Fedora', 'sudo dnf install ./muninn-2.20.1-1.x86_64.rpm'),
  ('Arch Linux', 'sudo pacman -U muninn-bin-2.20.1-1-x86_64.pkg.tar.zst'),
  (
    'Release tarball (Linux, macOS)',
    'tar -xzf muninn-2.20.1-x86_64-unknown-linux-gnu.tar.gz\nmkdir -p ~/.local/bin\ninstall -m755 muninn-2.20.1-x86_64-unknown-linux-gnu/mnnn ~/.local/bin/mnnn',
  ),
];

const docPages = [
  ('Getting around the TUI', 'tui.md'),
  ('Command line', 'cli.md'),
  ('Importing data', 'import.md'),
  ('Live searches', 'live-searches.md'),
  ('Journals', 'journals.md'),
  ('Models', 'models.md'),
  ('Code spaces', 'code-spaces.md'),
  ('Pipelines', 'pipelines.md'),
  ('Rules', 'rules.md'),
  ('Organizations and hosting', 'org-hosting.md'),
  ('Org site templates', 'org-templates.md'),
  ('Notifications', 'notifications.md'),
  ('Network and sync', 'network.md'),
  ('Live collaboration', 'collaboration.md'),
  ('MCP', 'mcp.md'),
  ('Encryption', 'encryption.md'),
  ('Themes', 'themes.md'),
];
