import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:muninn/src/api.dart';
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
  await t.pumpWidget(UncontrolledProviderScope(container: c, child: const MuninnApp()));
  await t.pumpAndSettle();
  return (api, c);
}

void main() {
  testWidgets('Keys menu opens status, not a placeholder', (t) async {
    final (_, c) = await pump(t);
    await t.tap(find.text('Keys'));
    await t.pumpAndSettle();
    await t.tap(find.text('Status…'));
    await t.pumpAndSettle();
    expect(c.read(sectionProvider), Section.keys);
    expect(find.textContaining('Coming to the app'), findsNothing);
    expect(find.textContaining('Not in the app yet'), findsNothing);
    expect(find.byKey(const ValueKey('keys-db-id')), findsOneWidget);
    expect(find.text('db-aaaa-bbbb'), findsOneWidget);
    expect(find.textContaining('v1'), findsWidgets);
    expect(find.textContaining('active'), findsOneWidget);
  });

  testWidgets('uninitialized status points at keys init', (t) async {
    final (api, c) = await pump(t);
    api.keys = const KeysStatus();
    c.read(sectionProvider.notifier).state = Section.keys;
    await t.pumpAndSettle();
    expect(find.byKey(const ValueKey('keys-uninitialized')), findsOneWidget);
    expect(find.textContaining('mnnn keys init'), findsOneWidget);
    expect(find.text('db-aaaa-bbbb'), findsNothing);
  });
}
