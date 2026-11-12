/// The app shell. Desktop (wide): the big header (a frameless window's custom
/// title bar, with window controls), then the TUI's menu bar (File, View, Data,
/// Projects, Net, Orgs, Keys, Theme, Help) and the main view. Phone (narrow): a compact
/// header, the main view in a safe area and a bottom navigation bar with large targets.
/// On a desktop window the custom window controls stay visible in the narrow layout too.
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'about_dialog.dart';
import 'io_dialogs.dart';
import 'shortcuts.dart';
import 'logo.dart';
import 'prefs_dialog.dart';
import 'state.dart';
import 'theme/muninn_theme.dart';
import 'time_range.dart';
import 'views/catalog.dart';
import 'views/backup.dart';
import 'views/collections.dart';
import 'views/filters.dart';
import 'views/theme_editor.dart';
import 'views/project_compute.dart';
import 'views/schedules_plugins.dart';
import 'views/create_then_share.dart';
import 'views/project_collab.dart';
import 'views/chart.dart';
import 'views/data.dart';
import 'views/help.dart';
import 'views/home.dart';
import 'views/net.dart';
import 'views/orgs.dart';
import 'views/orgs_dialogs.dart';
import 'views/orgs_rest.dart';
import 'views/keys.dart';
import 'views/keys_dialogs.dart';
import 'views/shared_actions.dart';
import 'views/net_dialogs.dart';
import 'views/project_extras.dart';
import 'views/projects.dart';
import 'workspace.dart';
import 'tiling_extras.dart';
import 'window_actions.dart';
import 'window_chrome.dart';
import 'window_controls.dart';

/// Widths below this use the phone layout.
const phoneBreakpoint = 720.0;

/// TUI header size. `auto` is compact on a phone and large on a wide window.
bool headerCompact(String mode, {required bool phone}) => switch (mode) {
  'compact' => true,
  'large' => false,
  _ => phone,
};

class AppShell extends ConsumerWidget {
  const AppShell({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final section = ref.watch(sectionProvider);
    return LayoutBuilder(
      builder: (context, c) {
        final phone = c.maxWidth < phoneBreakpoint;
        final compact = headerCompact(
          ref.watch(headerModeProvider),
          phone: phone,
        );
        final body = Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const MigrateBanner(),
            const RangeBar(),
            Expanded(child: sectionView(section)),
          ],
        );
        if (phone) {
          final chrome = ref.watch(windowChromePolicyProvider);
          final windowActions = ref.watch(windowActionsProvider);
          final phoneActions = <Widget>[
            PopupMenuButton<String>(
              tooltip: 'File',
              icon: const Icon(Icons.folder_outlined, size: 28),
              onSelected: (v) {
                if (v == 'import') showImportDialog(context, ref);
                if (v == 'export') showExportDialog(context, ref);
                if (v == 'prefs') showPreferencesDialog(context, ref);
                if (v == 'lock') lockStore(ref);
                if (v == 'backup') showBackupCreate(context, ref);
                if (v == 'restore') showBackupRestore(context);
              },
              itemBuilder: (_) => const [
                PopupMenuItem(value: 'import', child: Text('Import…')),
                PopupMenuItem(value: 'export', child: Text('Export…')),
                PopupMenuItem(value: 'prefs', child: Text('Preferences…')),
                PopupMenuItem(value: 'lock', child: Text('Lock')),
                PopupMenuItem(value: 'backup', child: Text('Backup…')),
                PopupMenuItem(value: 'restore', child: Text('Restore backup…')),
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
          ];
          final title = FittedBox(
            fit: BoxFit.scaleDown,
            alignment: Alignment.centerLeft,
            child: _Brand(compact: compact),
          );
          final PreferredSizeWidget appBar = chrome.enabled
              ? PreferredSize(
                  preferredSize: Size.fromHeight(compact ? 64 : 96),
                  child: Material(
                    color: Theme.of(context).colorScheme.surface,
                    child: SizedBox(
                      height: compact ? 64 : 96,
                      child: MuninnTitleBar(
                        policy: chrome,
                        actions: windowActions,
                        child: Row(
                          children: [
                            Expanded(child: title),
                            ...phoneActions,
                          ],
                        ),
                      ),
                    ),
                  ),
                )
              : AppBar(
                  toolbarHeight: compact ? 64 : 96,
                  title: title,
                  actions: phoneActions,
                );
          return MuninnShortcuts(
            child: Scaffold(
              appBar: appBar,
              body: SafeArea(child: body),
              bottomNavigationBar: NavigationBar(
                selectedIndex: _phoneTabs.indexOf(_phoneTabFor(section)),
                onDestinationSelected: (i) =>
                    ref.read(sectionProvider.notifier).state = _phoneTabs[i],
                destinations: const [
                  NavigationDestination(
                    icon: Icon(Icons.home_outlined),
                    selectedIcon: Icon(Icons.home),
                    label: 'Home',
                  ),
                  NavigationDestination(
                    icon: Icon(Icons.candlestick_chart_outlined),
                    label: 'Data',
                  ),
                  NavigationDestination(
                    icon: Icon(Icons.folder_outlined),
                    label: 'Projects',
                  ),
                  NavigationDestination(
                    icon: Icon(Icons.hub_outlined),
                    label: 'Net',
                  ),
                  NavigationDestination(
                    icon: Icon(Icons.more_horiz),
                    label: 'More',
                  ),
                ],
              ),
            ),
          );
        }
        return MuninnShortcuts(
          child: Scaffold(
            body: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                _Header(compact: compact),
                const MuninnMenuBar(),
                Expanded(child: body),
              ],
            ),
          ),
        );
      },
    );
  }
}

const _phoneTabs = [
  Section.home,
  Section.data,
  Section.projects,
  Section.net,
  Section.help,
];
Section _phoneTabFor(Section s) => _phoneTabs.contains(s) ? s : Section.help;

Widget sectionView(Section s) => switch (s) {
  Section.home => const HomeView(),
  Section.data => const DataView(),
  Section.projects => const ProjectsView(),
  Section.net => const NetView(),
  Section.orgs => const OrgsView(),
  Section.keys => const KeysView(),
  Section.help => const HelpView(),
};

/// The website's bombastic header: the raven and a huge gradient wordmark.
class _Header extends ConsumerWidget {
  const _Header({required this.compact});
  final bool compact;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final m = context.muninn;
    final chrome = ref.watch(windowChromePolicyProvider);
    final actions = ref.watch(windowActionsProvider);
    return Container(
      padding: EdgeInsets.fromLTRB(
        chrome.enabled && chrome.side == WindowControlSide.left ? 8 : 20,
        14,
        chrome.enabled && chrome.side == WindowControlSide.right ? 0 : 20,
        10,
      ),
      decoration: BoxDecoration(
        gradient: LinearGradient(
          colors: [m.bg, Color.lerp(m.bg, m.header, 0.12)!, m.bg],
        ),
        border: Border(bottom: BorderSide(color: m.border)),
      ),
      child: MuninnTitleBar(
        policy: chrome,
        actions: actions,
        child: _Brand(compact: compact),
      ),
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
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        RavenLogo(size: size),
        const SizedBox(width: 12),
        ShaderMask(
          shaderCallback: (r) =>
              LinearGradient(colors: [m.header, m.accent]).createShader(r),
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
          Text(
            'v${about.version}',
            style: TextStyle(color: m.muted, fontSize: 14),
          ),
        ],
      ],
    );
  }
}

/// The TUI's menu bar, item for item where the app has the view; the rest say where to go.
class MuninnMenuBar extends ConsumerStatefulWidget {
  const MuninnMenuBar({super.key});
  @override
  ConsumerState<MuninnMenuBar> createState() => _MuninnMenuBarState();
}

class _MuninnMenuBarState extends ConsumerState<MuninnMenuBar> {
  final _fileMenu = MenuController();

  @override
  Widget build(BuildContext context) {
    ref.listen<int>(menuBarRequestProvider, (previous, next) {
      if (previous != next) _fileMenu.open();
    });
    void go(Section s) => ref.read(sectionProvider.notifier).state = s;
    MenuItemButton item(
      String label,
      VoidCallback onTap, [
      MenuSerializableShortcut? shortcut,
    ]) => MenuItemButton(
      onPressed: onTap,
      shortcut: shortcut,
      child: Text(label),
    );
    SubmenuButton menu(
      String label,
      List<Widget> children, {
      MenuController? controller,
      String? accelerator,
    }) => SubmenuButton(
      controller: controller,
      menuChildren: children,
      child: DefaultTextStyle.merge(
        style: const TextStyle(fontWeight: FontWeight.w600),
        child: MenuAcceleratorLabel(accelerator ?? '&$label'),
      ),
    );
    final names = ref.watch(themeNamesProvider).value ?? const <String>[];
    final sel = ref.watch(themeSelectionProvider);
    return MenuBar(
      children: [
        menu('File', [
          item('Home', () => go(Section.home)),
          item('Lock', () => lockStore(ref)),
          item(
            'Import…',
            () => showImportDialog(context, ref),
            const SingleActivator(LogicalKeyboardKey.keyI),
          ),
          item('Export…', () => showExportDialog(context, ref)),
          SubmenuButton(
            menuChildren: [
              item('Create…', () => showBackupCreate(context, ref)),
              item('Restore…', () => showBackupRestore(context)),
            ],
            child: const Text('Backup'),
          ),
          item('Preferences…', () => showPreferencesDialog(context, ref)),
          item(
            'Authors…',
            () => showAuthorsDialog(context, ref),
            const SingleActivator(LogicalKeyboardKey.keyA, shift: true),
          ),
          const Divider(),
          item(
            'Quit',
            () => SystemNavigator.pop(),
            const SingleActivator(LogicalKeyboardKey.keyQ),
          ),
        ], controller: _fileMenu),
        menu('View', [
          SubmenuButton(
            menuChildren: [
              item('List', () {
                ref.read(sectionProvider.notifier).state = Section.data;
                applyWorkspace(
                  ref,
                  ref
                      .read(workspaceProvider)
                      .split(
                        ref.read(workspaceProvider).newDir,
                        ref.read(workspaceAreaProvider),
                        asChart: false,
                      ),
                  context: context,
                );
              }),
              item('Chart', () {
                ref.read(sectionProvider.notifier).state = Section.data;
                applyWorkspace(
                  ref,
                  ref
                      .read(workspaceProvider)
                      .split(
                        ref.read(workspaceProvider).newDir,
                        ref.read(workspaceAreaProvider),
                        asChart: true,
                      ),
                  context: context,
                );
              }),
            ],
            child: const Text('New tile'),
          ),
          SubmenuButton(
            menuChildren: [
              item('Right', () {
                ref.read(sectionProvider.notifier).state = Section.data;
                applyWorkspace(
                  ref,
                  ref
                      .read(workspaceProvider)
                      .split(
                        SplitDir.horizontal,
                        ref.read(workspaceAreaProvider),
                      ),
                  context: context,
                );
              }),
              item('Below', () {
                ref.read(sectionProvider.notifier).state = Section.data;
                applyWorkspace(
                  ref,
                  ref
                      .read(workspaceProvider)
                      .split(
                        SplitDir.vertical,
                        ref.read(workspaceAreaProvider),
                      ),
                  context: context,
                );
              }),
            ],
            child: const Text('Split'),
          ),
          item('Close tile', () {
            ref.read(sectionProvider.notifier).state = Section.data;
            applyWorkspace(
              ref,
              ref.read(workspaceProvider).closeTile(),
              context: context,
            );
          }),
          item('Zoom tile', () {
            ref.read(sectionProvider.notifier).state = Section.data;
            setWorkspace(ref, ref.read(workspaceProvider).toggleZoom());
          }),
          item('Equalize', () {
            ref.read(sectionProvider.notifier).state = Section.data;
            setWorkspace(ref, ref.read(workspaceProvider).equalize());
          }),
          item('Change view', () {
            ref.read(sectionProvider.notifier).state = Section.data;
            toggleDataView(ref);
          }),
          item('Link tile', () {
            ref.read(sectionProvider.notifier).state = Section.data;
            setWorkspace(ref, ref.read(workspaceProvider).cycleLink());
          }),
          item(
            'Chart layers…',
            () => showChartLayersSheet(context, ref),
            const SingleActivator(LogicalKeyboardKey.keyL, shift: true),
          ),
          SubmenuButton(
            menuChildren: [
              item('Split', () {
                ref.read(sectionProvider.notifier).state = Section.data;
                applyWorkspace(
                  ref,
                  ref.read(workspaceProvider).setContainer(ContainerMode.split),
                  context: context,
                );
              }),
              item('Tabbed', () {
                ref.read(sectionProvider.notifier).state = Section.data;
                applyWorkspace(
                  ref,
                  ref
                      .read(workspaceProvider)
                      .setContainer(ContainerMode.tabbed),
                  context: context,
                );
              }),
              item('Stacked', () {
                ref.read(sectionProvider.notifier).state = Section.data;
                applyWorkspace(
                  ref,
                  ref
                      .read(workspaceProvider)
                      .setContainer(ContainerMode.stacked),
                  context: context,
                );
              }),
            ],
            child: const Text('Container'),
          ),
          SubmenuButton(
            menuChildren: [
              item('2-up', () {
                ref.read(sectionProvider.notifier).state = Section.data;
                setWorkspace(
                  ref,
                  ref.read(workspaceProvider).preset(LayoutPreset.twoUp),
                );
              }),
              item('3-column', () {
                ref.read(sectionProvider.notifier).state = Section.data;
                setWorkspace(
                  ref,
                  ref.read(workspaceProvider).preset(LayoutPreset.threeColumn),
                );
              }),
              item('Main + stack', () {
                ref.read(sectionProvider.notifier).state = Section.data;
                setWorkspace(
                  ref,
                  ref.read(workspaceProvider).preset(LayoutPreset.mainStack),
                );
              }),
              item('2×2', () {
                ref.read(sectionProvider.notifier).state = Section.data;
                setWorkspace(
                  ref,
                  ref.read(workspaceProvider).preset(LayoutPreset.grid2x2),
                );
              }),
              const Divider(),
              item('Save current…', () => showSaveLayoutDialog(context, ref)),
              item('Saved…', () => showSavedLayoutsDialog(context, ref)),
            ],
            child: const Text('Layouts'),
          ),
          item(
            'History',
            () => toggleHistory(ref),
            const SingleActivator(LogicalKeyboardKey.keyY, shift: true),
          ),
          item(
            'Step back',
            () => stepHistory(ref, back: true).catchError((_) {}),
            const SingleActivator(LogicalKeyboardKey.keyU),
          ),
          item(
            'Step forward',
            () => stepHistory(ref, back: false).catchError((_) {}),
            const SingleActivator(LogicalKeyboardKey.keyU, shift: true),
          ),
          const Divider(),
          item('Toggle sidebar', () {
            ref.read(sectionProvider.notifier).state = Section.data;
            setWorkspace(ref, ref.read(workspaceProvider).toggleSidebar());
          }),
          item(
            'Refresh',
            () => refreshMuninn(ref),
            const SingleActivator(LogicalKeyboardKey.f5),
          ),
        ]),
        menu('Data', [
          item('Datasets', () => go(Section.data)),
          SubmenuButton(
            menuChildren: [
              item('Source…', () => showSourceFilter(context, ref)),
              item('Symbol…', () => showSymbolFilter(context, ref)),
              MenuItemButton(
                shortcut: const SingleActivator(
                  LogicalKeyboardKey.keyT,
                  shift: true,
                ),
                onPressed: () => showTimeRangePicker(context, ref),
                child: const Text('Time range…'),
              ),
              item('Price…', () => showPriceFilter(context, ref)),
              item('Period…', () => showPeriodFilter(context, ref)),
              item('Year…', () => showYearFilter(context, ref)),
              item(
                'Clear all',
                () => clearDataFilters(ref),
                const SingleActivator(LogicalKeyboardKey.keyX, shift: true),
              ),
            ],
            child: const Text('Filters'),
          ),
          SubmenuButton(
            menuChildren: [
              item('Year', () => setSort(ref, sort: 'year')),
              item('Name', () => setSort(ref, sort: 'name')),
              item('Asc/Desc', () {
                final cur = ref.read(dataFiltersProvider);
                setSort(ref, desc: !cur.desc);
              }),
            ],
            child: const Text('Sort'),
          ),
          item('Coverage & gaps…', () => showCoverage(context, ref)),
          item('Sources…', () => showSourcesDialog(context, ref)),
          item('Symbols…', () => showSymbolsDialog(context, ref)),
          item('Models…', () {
            ref.read(projectPaneProvider.notifier).state = ProjectPane.models;
            go(Section.projects);
          }, const SingleActivator(LogicalKeyboardKey.keyD, shift: true)),
        ]),
        menu('Projects', [
          item('Projects…', () {
            ref.read(projectPaneProvider.notifier).state = ProjectPane.projects;
            go(Section.projects);
          }, const SingleActivator(LogicalKeyboardKey.keyP, shift: true)),
          item('New project…', () => showNewProjectDialog(context, ref)),
          item('Live searches…', () {
            ref.read(projectPaneProvider.notifier).state = ProjectPane.searches;
            go(Section.projects);
          }, const SingleActivator(LogicalKeyboardKey.keyF, shift: true)),
          item(
            'Save search as live search…',
            () => showSaveSearchDialog(context, ref),
          ),
          item('Journals…', () {
            ref.read(projectPaneProvider.notifier).state = ProjectPane.journals;
            go(Section.projects);
          }, const SingleActivator(LogicalKeyboardKey.keyJ, shift: true)),
          item('New journal…', () => showNewJournalDialog(context, ref)),
          item('Reports…', () {
            ref.read(projectPaneProvider.notifier).state = ProjectPane.reports;
            go(Section.projects);
          }),
          item('Models…', () {
            ref.read(projectPaneProvider.notifier).state = ProjectPane.models;
            go(Section.projects);
          }),
          item('New report…', () => showNewReportDialog(context, ref)),
          item('Import archive…', () => showImportProjectDialog(context, ref)),
          item('Code spaces…', () {
            ref.read(projectPaneProvider.notifier).state = ProjectPane.spaces;
            go(Section.projects);
          }),
          item('Rules…', () {
            ref.read(projectPaneProvider.notifier).state = ProjectPane.rules;
            go(Section.projects);
          }, const SingleActivator(LogicalKeyboardKey.keyR, shift: true)),
          item('New rule…', () => showNewRuleDialog(context, ref)),
          item('Rule sets…', () {
            ref.read(projectPaneProvider.notifier).state = ProjectPane.rulesets;
            go(Section.projects);
          }),
          item('New rule set…', () => showNewRulesetDialog(context, ref)),
          item('Schedules…', () {
            ref.read(projectPaneProvider.notifier).state =
                ProjectPane.schedules;
            go(Section.projects);
          }),
          item('New schedule…', () => showNewScheduleDialog(context, ref)),
          item(
            'Create then share…',
            () => showCreateThenShareDialog(context, ref),
          ),
          item(
            '+ New review subject…',
            () => showCreateReviewSubjectDialog(context, ref),
          ),
          item('Reviews…', () {
            ref.read(projectPaneProvider.notifier).state = ProjectPane.reviews;
            go(Section.projects);
          }, const SingleActivator(LogicalKeyboardKey.keyV, shift: true)),
          item('New review…', () => showNewReviewDialog(context, ref)),
          item('Notifications…', () {
            ref.read(projectPaneProvider.notifier).state =
                ProjectPane.notifications;
            go(Section.projects);
          }),
          item(
            'Send a notification…',
            () => showSendNotificationDialog(context, ref),
          ),
        ]),
        menu('Code', [
          item('Code…', () {
            ref.read(projectPaneProvider.notifier).state = ProjectPane.code;
            go(Section.projects);
          }, const SingleActivator(LogicalKeyboardKey.keyC, shift: true)),
          item('New code…', () => showNewCodeSpaceDialog(context, ref)),
          item('Code spaces…', () {
            ref.read(projectPaneProvider.notifier).state = ProjectPane.spaces;
            go(Section.projects);
          }),
          item('New code space…', () => showNewCollectionDialog(context, ref)),
          item('Migrate…', () => showMigrateDialog(context, ref)),
          item('Live pipes…', () {
            ref.read(projectPaneProvider.notifier).state = ProjectPane.pipes;
            go(Section.projects);
          }),
          item('Run a pipe…', () => showRunPipeDialog(context, ref)),
          item('Pipelines…', () {
            ref.read(projectPaneProvider.notifier).state =
                ProjectPane.pipelines;
            go(Section.projects);
          }, const SingleActivator(LogicalKeyboardKey.keyE, shift: true)),
          item('New pipeline…', () => showNewPipelineDialog(context, ref)),
        ]),
        menu('Plug', [
          item('Parsers…', () => showPluginsPane(context, ref, kind: 'parser')),
          item(
            'Transforms…',
            () => showPluginsPane(context, ref, kind: 'transform'),
          ),
          item(
            'Analysis…',
            () => showPluginsPane(context, ref, kind: 'analysis'),
          ),
          item(
            'Ingest connectors…',
            () => showPluginsPane(context, ref, kind: 'ingest-connector'),
          ),
          item('All plugins…', () => showPluginsPane(context, ref)),
          item('Install plugin…', () => showPluginInstallDialog(context, ref)),
        ], accelerator: 'P&lug'),
        menu('Net', [
          item('Status…', () {
            ref.read(netPaneProvider.notifier).state = NetPane.node;
            go(Section.net);
          }, const SingleActivator(LogicalKeyboardKey.keyK, shift: true)),
          item('Start node', () => netStartNode(context, ref)),
          item('Stop node', () => netStopNode(context, ref)),
          item('Settings…', () {
            ref.read(netPaneProvider.notifier).state = NetPane.settings;
            go(Section.net);
          }),
          item('Connections…', () {
            ref.read(netPaneProvider.notifier).state = NetPane.connections;
            go(Section.net);
          }),
          item('Invites…', () {
            ref.read(netPaneProvider.notifier).state = NetPane.invites;
            go(Section.net);
          }),
          item(
            'Invite or connect…',
            () => showNetInviteOrConnectDialog(context, ref),
          ),
          item('Offer token…', () => showExchangeOfferDialog(context, ref)),
          item('Accept token…', () => showExchangeAcceptDialog(context, ref)),
          item('Nodes…', () {
            ref.read(netPaneProvider.notifier).state = NetPane.exchanges;
            go(Section.net);
          }),
          item(
            'Export exchanges…',
            () => showNetExchangeExportDialog(context, ref),
          ),
          item(
            'Restore exchanges…',
            () => showNetExchangeRestoreDialog(context, ref),
          ),
          item('Peers…', () {
            ref.read(netPaneProvider.notifier).state = NetPane.peers;
            go(Section.net);
          }),
          item('Peers, messages & live…', () {
            ref.read(netPaneProvider.notifier).state = NetPane.collab;
            go(Section.net);
          }, const SingleActivator(LogicalKeyboardKey.keyW, shift: true)),
          item('MCP…', () {
            ref.read(netPaneProvider.notifier).state = NetPane.mcp;
            go(Section.net);
          }),
          item('Relays…', () {
            ref.read(netPaneProvider.notifier).state = NetPane.relays;
            go(Section.net);
          }),
          item('Shared data…', () {
            ref.read(netPaneProvider.notifier).state = NetPane.shared;
            go(Section.net);
          }),
          item('Network search…', () {
            ref.read(netPaneProvider.notifier).state = NetPane.search;
            go(Section.net);
          }),
          item('Broadcast…', () => showNetBroadcastDialog(context, ref)),
          item('Dark shards…', () {
            ref.read(netPaneProvider.notifier).state = NetPane.shards;
            go(Section.net);
          }),
        ]),
        menu('Orgs', [
          item(
            'Organizations…',
            () => go(Section.orgs),
            const SingleActivator(LogicalKeyboardKey.keyG, shift: true),
          ),
          item('New organization…', () => showOrgCreateDialog(context, ref)),
          item('Join…', () => showOrgJoinDialog(context, ref)),
          item('Apply…', () => showOrgApplyDialog(context, ref)),
          item('Browse…', () => showOrgBrowseDialog(context, ref)),
          item('Certificates…', () => showOrgCertsDialog(context, ref)),
          item('Let\'s Encrypt…', () => showOrgNodeAcmeDialog(context, ref)),
          item('Start site', () => orgHostStart(context, ref)),
          item('Stop site', () => orgHostStop(context, ref)),
          item('Announce site…', () => showOrgAnnounceDialog(context, ref)),
          item(
            'Announced sites…',
            () => showAnnouncedSitesDialog(context, ref),
          ),
        ]),
        menu('Keys', [
          item('Status…', () => go(Section.keys)),
          item('Initialize…', () => showKeysInitDialog(context, ref)),
          item('Lock', () => lockStore(ref)),
          item('Rotate service key', () => showKeysRotateDialog(context, ref)),
          item('Rekey database', () => showKeysRekeyDialog(context, ref)),
          item('Export…', () => showKeysExportDialog(context, ref)),
          SubmenuButton(
            menuChildren: [
              item(
                'From words',
                () => showKeysRecoverWordsDialog(context, ref),
              ),
              item(
                'From shards',
                () => showKeysRecoverShardsDialog(context, ref),
              ),
              item(
                'From a shard backup',
                () => showShardBackupRestoreDialog(context, ref),
              ),
            ],
            child: const Text('Recover'),
          ),
          item(
            'Re-issue shards',
            () => showKeysReissueShardsDialog(context, ref),
          ),
        ]),
        menu('Theme', [
          for (final n in names)
            MenuItemButton(
              leadingIcon: Icon(
                n == sel.name
                    ? Icons.radio_button_checked
                    : Icons.radio_button_unchecked,
                size: 16,
              ),
              onPressed: () => ref.read(themeSelectionProvider.notifier).set(n),
              child: Text(n),
            ),
          const Divider(),
          for (final mode in ['system', 'dark', 'light'])
            MenuItemButton(
              leadingIcon: Icon(
                mode == sel.mode ? Icons.check : null,
                size: 16,
              ),
              onPressed: () =>
                  ref.read(themeSelectionProvider.notifier).setMode(mode),
              child: Text('Mode: $mode'),
            ),
          const Divider(),
          item('Edit current…', () => showThemeEditor(context, ref)),
          item(
            'New from current…',
            () => showThemeEditor(context, ref, saveAs: true),
          ),
          item('Import…', () => importTheme(context, ref)),
          item('Export…', () => exportTheme(context, ref)),
        ]),
        menu('Help', [
          item(
            'Topics…',
            () => openHelp(ref),
            const SingleActivator(LogicalKeyboardKey.f1),
          ),
          item('Search help…', () => openHelp(ref, search: true)),
          item('Keybindings', () => openHelp(ref, topic: 'keybindings')),
          item('About Muninn', () => showMuninnAbout(context, ref)),
        ]),
      ],
    );
  }
}

/// Theme picker as a bottom sheet (phone).
Future<void> showThemeSheet(BuildContext context, WidgetRef ref) =>
    showModalBottomSheet(
      context: context,
      showDragHandle: true,
      builder: (ctx) => Consumer(
        builder: (ctx, ref, _) {
          final names = ref.watch(themeNamesProvider).value ?? const <String>[];
          final sel = ref.watch(themeSelectionProvider);
          return SafeArea(
            child: ListView(
              shrinkWrap: true,
              children: [
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 16),
                  child: SegmentedButton<String>(
                    segments: const [
                      ButtonSegment(value: 'system', label: Text('System')),
                      ButtonSegment(value: 'dark', label: Text('Dark')),
                      ButtonSegment(value: 'light', label: Text('Light')),
                    ],
                    selected: {sel.mode},
                    onSelectionChanged: (s) => ref
                        .read(themeSelectionProvider.notifier)
                        .setMode(s.first),
                  ),
                ),
                for (final n in names)
                  ListTile(
                    minTileHeight: 56,
                    title: Text(n),
                    trailing: n == sel.name ? const Icon(Icons.check) : null,
                    onTap: () =>
                        ref.read(themeSelectionProvider.notifier).set(n),
                  ),
              ],
            ),
          );
        },
      ),
    );

/// For actions the app doesn't have yet: says which `mnnn` command or TUI menu does it.
Future<void> showLater(
  BuildContext context,
  String what,
  String cli,
) => showDialog(
  context: context,
  builder: (ctx) => AlertDialog(
    title: Row(
      children: [
        Expanded(child: Text(what)),
        IconButton(
          tooltip: 'Close',
          icon: const Text('[X]'),
          onPressed: () => Navigator.pop(ctx),
        ),
      ],
    ),
    content: Text(
      'Not in the app yet. In the terminal: `$cli`, or the same menu in `mnnn tui`.',
    ),
  ),
);

/// Forget the key and the data shown. The selected dataset key stays (it is saved in
/// preferences and reopens after the next unlock); nothing is shown while locked.
void lockStore(WidgetRef ref) {
  ref.read(apiProvider).lock();
  ref.read(unlockedProvider.notifier).state = false;
  ref.read(selectedProjectProvider.notifier).state = null;
  ref.read(liveLayoutReadyProvider.notifier).state = false;
  ref.read(viewingModelIdProvider.notifier).state = null;
  ref.read(viewingRecordIdProvider.notifier).state = null;
}
