import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:muninn/src/app.dart';
import 'package:muninn/src/state.dart';

import 'fake_api.dart';

Future<(FakeApi, ProviderContainer)> pump(WidgetTester t) async {
  t.view.physicalSize = const Size(1280, 800);
  t.view.devicePixelRatio = 1;
  addTearDown(t.view.reset);
  final api = FakeApi();
  final c = ProviderContainer(overrides: [apiProvider.overrideWithValue(api)]);
  addTearDown(c.dispose);
  await t.pumpWidget(
    UncontrolledProviderScope(container: c, child: const MuninnApp()),
  );
  await t.pumpAndSettle();
  return (api, c);
}

Future<void> unlock(WidgetTester t) async {
  await t.enterText(find.byType(TextField), 'pw');
  await t.tap(find.text('Unlock'));
  await t.pumpAndSettle();
}

void main() {
  testWidgets('source filter hides the other dataset', (t) async {
    final (_, c) = await pump(t);
    c.read(sectionProvider.notifier).state = Section.data;
    await t.pumpAndSettle();
    await unlock(t);
    expect(find.text('ETH/USDT · 1h'), findsOneWidget);
    await t.tap(find.text('Data'));
    await t.pumpAndSettle();
    await t.tap(find.text('Filters'));
    await t.pumpAndSettle();
    await t.tap(find.text('Source…'));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('filter-Source-coinbase')));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('filter-Source-apply')));
    await t.pumpAndSettle();
    expect(find.text('BTC/USD · 5m'), findsOneWidget);
    expect(find.text('ETH/USDT · 1h'), findsNothing);
  });

  testWidgets('backup restore names the command', (t) async {
    await pump(t);
    await t.tap(find.text('File'));
    await t.pumpAndSettle();
    await t.tap(find.text('Backup'));
    await t.pumpAndSettle();
    await t.tap(find.text('Restore…'));
    await t.pumpAndSettle();
    expect(find.textContaining('mnnn backup restore'), findsOneWidget);
  });

  testWidgets('theme editor saves the current theme', (t) async {
    final (api, _) = await pump(t);
    await t.tap(find.text('Theme'));
    await t.pumpAndSettle();
    await t.tap(find.text('Edit current…'));
    await t.pumpAndSettle();
    expect(find.byKey(const ValueKey('theme-role-bg')), findsOneWidget);
    await t.tap(find.byKey(const ValueKey('theme-save')));
    await t.pumpAndSettle();
    expect(api.lastSavedTheme, 'tokyo-night');
  });

  testWidgets('coverage lists a series', (t) async {
    final (_, c) = await pump(t);
    c.read(sectionProvider.notifier).state = Section.data;
    await t.pumpAndSettle();
    await unlock(t);
    await t.tap(find.text('Data'));
    await t.pumpAndSettle();
    await t.tap(find.text('Coverage & gaps…'));
    await t.pumpAndSettle();
    expect(find.textContaining('0 missing'), findsOneWidget);
  });
}
