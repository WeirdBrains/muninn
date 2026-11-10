import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'shell.dart';
import 'state.dart';
import 'tiling_extras.dart';
import 'theme/muninn_theme.dart';

class MuninnApp extends ConsumerStatefulWidget {
  const MuninnApp({super.key});
  @override
  ConsumerState<MuninnApp> createState() => _MuninnAppState();
}

class _MuninnAppState extends ConsumerState<MuninnApp>
    with WidgetsBindingObserver {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    WidgetsBinding.instance.addPostFrameCallback((_) async {
      _brightness();
      final api = ref.read(apiProvider);
      try {
        final s = await api.savedState();
        if (!mounted) return;
        ref.read(themeSelectionProvider.notifier)
          ..set(s.theme)
          ..setMode(s.mode);
        ref.read(timeRangeProvider.notifier).state = s.range;
        ref.read(selectedDatasetProvider.notifier).state = s.dataset;
        try {
          ref.read(dataFiltersProvider.notifier).state = await api
              .sidebarFilters();
          ref.read(headerModeProvider.notifier).state = await api.headerMode();
          ref.read(historyOpenProvider.notifier).state = await api
              .historyOpen();
        } catch (_) {
          /* defaults stay */
        }
      } catch (_) {
        /* defaults stay */
      }
      if (!mounted) return;
      // From here on every change is saved, whichever picker made it (desktop menus, phone sheet,
      // Shift+T, Projects): preferences.toml is shared with the TUI.
      void save(Future<void> Function() f) => f().catchError((_) {});
      ref.listenManual(themeSelectionProvider, (a, b) {
        if (a?.name != b.name) save(() => api.saveTheme(b.name));
        if (a?.mode != b.mode) save(() => api.saveThemeMode(b.mode));
      });
      ref.listenManual(timeRangeProvider, (a, b) {
        if (a != b) save(() => api.saveTimeRange(b));
      });
      ref.listenManual(dataFiltersProvider, (a, b) {
        if (a != b) save(() => api.saveSidebarFilters(b));
      });
      ref.listenManual(headerModeProvider, (a, b) {
        if (a != b) save(() => api.saveHeaderMode(b));
      });
      ref.listenManual(selectedDatasetProvider, (a, b) {
        if (a != b) save(() => api.saveDataset(b));
      });
      ref.listenManual(historyOpenProvider, (a, b) {
        if (a != b) save(() => api.setHistoryOpen(b));
      });
    });
  }

  void _brightness() => ref.read(platformBrightnessProvider.notifier).state =
      WidgetsBinding.instance.platformDispatcher.platformBrightness;

  @override
  void didChangePlatformBrightness() => _brightness();

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = ref.watch(resolvedThemeProvider);
    final theme = t.value;
    if (theme == null) {
      return const MaterialApp(
        debugShowCheckedModeBanner: false,
        home: Scaffold(body: Center(child: CircularProgressIndicator())),
      );
    }
    return MaterialApp(
      title: 'Muninn',
      debugShowCheckedModeBanner: false,
      theme: muninnThemeData(theme.roles, dark: theme.variant == 'dark'),
      home: const AppShell(),
    );
  }
}
