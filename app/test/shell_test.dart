import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:muninn/src/app.dart';
import 'package:muninn/src/state.dart';

import 'fake_api.dart';

Future<FakeApi> pump(WidgetTester t, Size size) async {
  t.view.physicalSize = size;
  t.view.devicePixelRatio = 1;
  addTearDown(t.view.reset);
  final api = FakeApi();
  await t.pumpWidget(ProviderScope(overrides: [apiProvider.overrideWithValue(api)], child: const MuninnApp()));
  await t.pumpAndSettle();
  return api;
}

void main() {
  testWidgets('desktop: header and the TUI menus', (t) async {
    await pump(t, const Size(1280, 800));
    expect(find.text('MUNINN'), findsOneWidget);
    for (final m in ['File', 'View', 'Data', 'Projects', 'Net', 'Orgs', 'Keys', 'Theme', 'Help']) {
      expect(find.text(m), findsOneWidget, reason: m);
    }
    expect(find.byType(NavigationBar), findsNothing);
  });

  testWidgets('phone: bottom navigation, no menu bar', (t) async {
    await pump(t, const Size(390, 844));
    expect(find.byType(NavigationBar), findsOneWidget);
    expect(find.byType(MenuBar), findsNothing);
    await t.tap(find.text('Net'));
    await t.pumpAndSettle();
    expect(find.textContaining('Coming to the app'), findsNothing);
    expect(find.text('Unlock'), findsOneWidget);
  });

  testWidgets('theme menu switches theme', (t) async {
    final api = await pump(t, const Size(1280, 800));
    await t.tap(find.text('Theme'));
    await t.pumpAndSettle();
    await t.tap(find.text('nord'));
    await t.pumpAndSettle();
    expect(api.loaded.last, 'nord/dark');
  });

  testWidgets('home unlocks and lists projects and datasets', (t) async {
    final api = await pump(t, const Size(1280, 800));
    await t.enterText(find.byType(TextField), 'pw');
    await t.tap(find.text('Unlock'));
    await t.pumpAndSettle();
    expect(api.lastPassword, 'pw');
    expect(find.text('Alpha'), findsOneWidget);
    expect(find.text('coinbase · BTC/USD · 5m'), findsOneWidget);
  });

  testWidgets('about dialog has a clickable [X]', (t) async {
    await pump(t, const Size(1280, 800));
    await t.tap(find.text('Help'));
    await t.pumpAndSettle();
    await t.tap(find.text('About Muninn'));
    await t.pumpAndSettle();
    expect(find.text('https://muninn.weirdbrains.com'), findsOneWidget);
    await t.tap(find.text('[X]'));
    await t.pumpAndSettle();
    expect(find.text('https://muninn.weirdbrains.com'), findsNothing);
  });
}
