import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'shell.dart';
import 'state.dart';
import 'theme/muninn_theme.dart';

class MuninnApp extends ConsumerStatefulWidget {
  const MuninnApp({super.key});
  @override
  ConsumerState<MuninnApp> createState() => _MuninnAppState();
}

class _MuninnAppState extends ConsumerState<MuninnApp> with WidgetsBindingObserver {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    WidgetsBinding.instance.addPostFrameCallback((_) async {
      _brightness();
      try {
        final c = await ref.read(apiProvider).currentTheme();
        ref.read(themeSelectionProvider.notifier)
          ..set(c.name)
          ..setMode(c.mode);
      } catch (_) {/* defaults stay */}
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
      return const MaterialApp(debugShowCheckedModeBanner: false, home: Scaffold(body: Center(child: CircularProgressIndicator())));
    }
    return MaterialApp(
      title: 'Muninn',
      debugShowCheckedModeBanner: false,
      theme: muninnThemeData(theme.roles, dark: theme.variant == 'dark'),
      home: const AppShell(),
    );
  }
}
