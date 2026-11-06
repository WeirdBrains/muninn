/// The app shell. Desktop (wide): the big header, then the TUI's menu bar (File, View, Data,
/// Projects, Net, Orgs, Keys, Theme, Help) and the main view. Phone (narrow): a compact
/// header, the main view in a safe area and a bottom navigation bar with large targets.
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'about_dialog.dart';
import 'api.dart';
import 'io_dialogs.dart';
import 'logo.dart';
import 'prefs_dialog.dart';
import 'state.dart';
import 'theme/muninn_theme.dart';
import 'time_range.dart';
import 'views/chart.dart';
import 'views/data.dart';
import 'views/home.dart';
import 'views/net.dart';
import 'views/orgs.dart';
import 'views/keys.dart';
import 'views/projects.dart';
import 'views/placeholder.dart';

/// Widths below this use the phone layout.
const phoneBreakpoint = 720.0;

class AppShell extends ConsumerWidget {
  const AppShell({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final section = ref.watch(sectionProvider);
    return LayoutBuilder(builder: (context, c) {
      final phone = c.maxWidth < phoneBreakpoint;
      final body = _withShortcuts(context, ref, Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        const RangeBar(),
        Expanded(child: sectionView(section)),
      ]));
      if (phone) {
        return Scaffold(
          appBar: AppBar(
            toolbarHeight: 64,
            title: const FittedBox(fit: BoxFit.scaleDown, alignment: Alignment.centerLeft, child: _Brand(compact: true)),
            actions: [
              PopupMenuButton<String>(
                tooltip: 'File',
                icon: const Icon(Icons.folder_outlined, size: 28),
                onSelected: (v) {
                  if (v == 'import') showImportDialog(context, ref);
                  if (v == 'export') showExportDialog(context, ref);
                  if (v == 'prefs') showPreferencesDialog(context, ref);
                  if (v == 'lock') lockStore(ref);
                },
                itemBuilder: (_) => const [
                  PopupMenuItem(value: 'import', child: Text('Import…')),
                  PopupMenuItem(value: 'export', child: Text('Export…')),
                  PopupMenuItem(value: 'prefs', child: Text('Preferences…')),
                  PopupMenuItem(value: 'lock', child: Text('Lock')),
                ],
              ),
              IconButton(
                tooltip: 'Theme',
                iconSize: 28,
                icon: const Icon(Icons.palette_outlined),
                onPressed: () => showThemeSheet(context, ref),
              ),
              IconButton(
                tooltip: 'About Muninn',
                iconSize: 28,
                icon: const Icon(Icons.info_outline),
                onPressed: () => showMuninnAbout(context, ref),
              ),
            ],
          ),
          body: SafeArea(child: body),
          bottomNavigationBar: NavigationBar(
            selectedIndex: _phoneTabs.indexOf(_phoneTabFor(section)),
            onDestinationSelected: (i) => ref.read(sectionProvider.notifier).state = _phoneTabs[i],
            destinations: const [
              NavigationDestination(icon: Icon(Icons.home_outlined), selectedIcon: Icon(Icons.home), label: 'Home'),
              NavigationDestination(icon: Icon(Icons.candlestick_chart_outlined), label: 'Data'),
              NavigationDestination(icon: Icon(Icons.folder_outlined), label: 'Projects'),
              NavigationDestination(icon: Icon(Icons.hub_outlined), label: 'Net'),
              NavigationDestination(icon: Icon(Icons.more_horiz), label: 'More'),
            ],
          ),
        );
      }
      return Scaffold(
        body: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const _Header(),
            const MuninnMenuBar(),
            Expanded(child: body),
          ],
        ),
      );
    });
  }
}

const _phoneTabs = [Section.home, Section.data, Section.projects, Section.net, Section.help];
Section _phoneTabFor(Section s) => _phoneTabs.contains(s) ? s : Section.help;

Widget sectionView(Section s) => switch (s) {
      Section.home => const HomeView(),
      Section.data => const DataView(),
      Section.projects => const ProjectsView(),
      Section.net => const NetView(),
      Section.orgs => const OrgsView(),
      Section.keys => const KeysView(),
      Section.help => const PlaceholderView(title: 'Help', cli: 'mnnn help', icon: Icons.help_outline),
    };

/// The website's bombastic header: the raven and a huge gradient wordmark.
class _Header extends ConsumerWidget {
  const _Header();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    return Container(
      padding: const EdgeInsets.fromLTRB(20, 14, 20, 10),
      decoration: BoxDecoration(
        gradient: LinearGradient(colors: [m.bg, Color.lerp(m.bg, m.header, 0.12)!, m.bg]),
        border: Border(bottom: BorderSide(color: m.border)),
      ),
      child: const _Brand(compact: false),
    );
  }
}

class _Brand extends ConsumerWidget {
  const _Brand({required this.compact});
  final bool compact;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final about = ref.watch(apiProvider).about();
    final size = compact ? 36.0 : 64.0;
    return Row(mainAxisSize: MainAxisSize.min, children: [
      RavenLogo(size: size),
      const SizedBox(width: 12),
      ShaderMask(
        shaderCallback: (r) => LinearGradient(colors: [m.header, m.accent]).createShader(r),
        child: Text(
          'MUNINN',
          style: TextStyle(
            fontSize: compact ? 26 : 44,
            fontWeight: FontWeight.w900,
            letterSpacing: compact ? 4 : 8,
            color: Colors.white,
            height: 1,
          ),
        ),
      ),
      if (!compact) ...[
        const SizedBox(width: 10),
        Text('v${about.version}', style: TextStyle(color: m.muted, fontSize: 14)),
      ],
    ]);
  }
}

/// The TUI's menu bar, item for item where the app has the view; the rest say where to go.
class MuninnMenuBar extends ConsumerWidget {
  const MuninnMenuBar({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    void go(Section s) => ref.read(sectionProvider.notifier).state = s;
    void later(String what, String cli) => showLater(context, what, cli);
    MenuItemButton item(String label, VoidCallback onTap) => MenuItemButton(onPressed: onTap, child: Text(label));
    SubmenuButton menu(String label, List<Widget> children) =>
        SubmenuButton(menuChildren: children, child: Text(label, style: const TextStyle(fontWeight: FontWeight.w600)));
    final names = ref.watch(themeNamesProvider).value ?? const <String>[];
    final sel = ref.watch(themeSelectionProvider);
    return MenuBar(children: [
      menu('File', [
        item('Home', () => go(Section.home)),
        item('Lock', () => lockStore(ref)),
        item('Import…', () => showImportDialog(context, ref)),
        item('Export…', () => showExportDialog(context, ref)),
        item('Preferences…', () => showPreferencesDialog(context, ref)),
      ]),
      menu('View', [
        item('Change view (chart / list)', () => ref.read(dataAsChartProvider.notifier).state = !ref.read(dataAsChartProvider)),
        item('Chart layers…', () => showChartLayersSheet(context, ref)),
        item('Refresh', () { ref.invalidate(overviewProvider); ref.invalidate(netOverviewProvider); ref.invalidate(orgOverviewProvider); ref.invalidate(keysStatusProvider); }),
      ]),
      menu('Data', [
        item('Datasets', () => go(Section.data)),
        SubmenuButton(menuChildren: [
          MenuItemButton(
            shortcut: const SingleActivator(LogicalKeyboardKey.keyT, shift: true),
            onPressed: () => showTimeRangePicker(context, ref),
            child: const Text('Time range…'),
          ),
          item('Clear time range', () => ref.read(timeRangeProvider.notifier).state = TimeRange.all),
        ], child: const Text('Filters')),
        item('Sources…', () => later('Sources', 'mnnn sources')),
        item('Symbols…', () => later('Symbols', 'mnnn symbols')),
      ]),
      menu('Projects', [
        item('Projects…', () => go(Section.projects)),
        item('New project…', () => showNewProjectDialog(context, ref)),
      ]),
      menu('Net', [
        item('Status…', () => go(Section.net)),
        item('Peers…', () => go(Section.net)),
      ]),
      menu('Orgs', [item('Organizations…', () => go(Section.orgs))]),
      menu('Keys', [item('Status…', () => go(Section.keys))]),
      menu('Theme', [
        for (final n in names)
          MenuItemButton(
            leadingIcon: Icon(n == sel.name ? Icons.radio_button_checked : Icons.radio_button_unchecked, size: 16),
            onPressed: () => ref.read(themeSelectionProvider.notifier).set(n),
            child: Text(n),
          ),
        const Divider(),
        for (final mode in ['system', 'dark', 'light'])
          MenuItemButton(
            leadingIcon: Icon(mode == sel.mode ? Icons.check : null, size: 16),
            onPressed: () => ref.read(themeSelectionProvider.notifier).setMode(mode),
            child: Text('Mode: $mode'),
          ),
      ]),
      menu('Help', [
        item('Topics…', () => go(Section.help)),
        item('About Muninn', () => showMuninnAbout(context, ref)),
      ]),
    ]);
  }
}

/// Theme picker as a bottom sheet (phone).
Future<void> showThemeSheet(BuildContext context, WidgetRef ref) => showModalBottomSheet(
      context: context,
      showDragHandle: true,
      builder: (ctx) => Consumer(builder: (ctx, ref, _) {
        final names = ref.watch(themeNamesProvider).value ?? const <String>[];
        final sel = ref.watch(themeSelectionProvider);
        return SafeArea(
          child: ListView(shrinkWrap: true, children: [
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16),
              child: SegmentedButton<String>(
                segments: const [
                  ButtonSegment(value: 'system', label: Text('System')),
                  ButtonSegment(value: 'dark', label: Text('Dark')),
                  ButtonSegment(value: 'light', label: Text('Light')),
                ],
                selected: {sel.mode},
                onSelectionChanged: (s) => ref.read(themeSelectionProvider.notifier).setMode(s.first),
              ),
            ),
            for (final n in names)
              ListTile(
                minTileHeight: 56,
                title: Text(n),
                trailing: n == sel.name ? const Icon(Icons.check) : null,
                onTap: () => ref.read(themeSelectionProvider.notifier).set(n),
              ),
          ]),
        );
      }),
    );

/// For actions the app doesn't have yet: says which `mnnn` command or TUI menu does it.
Future<void> showLater(BuildContext context, String what, String cli) => showDialog(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Row(children: [
          Expanded(child: Text(what)),
          IconButton(tooltip: 'Close', icon: const Text('[X]'), onPressed: () => Navigator.pop(ctx)),
        ]),
        content: Text('Not in the app yet. In the terminal: `$cli`, or the same menu in `mnnn tui`.'),
      ),
    );

/// Forget the key and the data shown. The selected dataset key stays (it is saved in
/// preferences and reopens after the next unlock); nothing is shown while locked.
void lockStore(WidgetRef ref) {
  ref.read(apiProvider).lock();
  ref.read(unlockedProvider.notifier).state = false;
  ref.read(selectedProjectProvider.notifier).state = null;
}

/// `T` (Shift+t) opens the time range picker anywhere (as in the TUI), except while typing.
Widget _withShortcuts(BuildContext context, WidgetRef ref, Widget child) => Focus(
      autofocus: true,
      onKeyEvent: (node, e) {
        if (e is! KeyDownEvent || e.logicalKey != LogicalKeyboardKey.keyT || !HardwareKeyboard.instance.isShiftPressed) {
          return KeyEventResult.ignored;
        }
        final f = FocusManager.instance.primaryFocus?.context;
        if (f != null && (f.widget is EditableText || f.findAncestorWidgetOfExactType<EditableText>() != null)) {
          return KeyEventResult.ignored;
        }
        showTimeRangePicker(context, ref);
        return KeyEventResult.handled;
      },
      child: child,
    );
