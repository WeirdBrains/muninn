import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:muninn/src/api.dart';
import 'package:muninn/src/app.dart';
import 'package:muninn/src/state.dart';

import 'fake_api.dart';

Future<FakeApi> pump(WidgetTester t, {int pending = 0}) async {
  t.view.physicalSize = const Size(1280, 800);
  t.view.devicePixelRatio = 1;
  addTearDown(t.view.reset);
  final api = FakeApi()
    ..migratePendingCount = pending
    ..collectionList = [
      const CollectionInfo(
        id: 'sp1',
        name: 'alpha',
        kind: 'analysis',
        main: 'alpha',
        members: 1,
      ),
    ]
    ..memberList = [
      const SpaceMemberInfo(
        codeId: 'c1',
        codeName: 'alpha',
        alias: 'alpha',
        main: true,
      ),
    ];
  final c = ProviderContainer(overrides: [apiProvider.overrideWithValue(api)]);
  addTearDown(c.dispose);
  await t.pumpWidget(
    UncontrolledProviderScope(container: c, child: const MuninnApp()),
  );
  await t.pumpAndSettle();
  await t.enterText(find.byType(TextField), 'pw');
  await t.tap(find.text('Unlock'));
  await t.pumpAndSettle();
  return api;
}

void main() {
  testWidgets('Code menu opens collections and members', (t) async {
    await pump(t);
    await t.tap(find.text('Code'));
    await t.pumpAndSettle();
    await t.tap(find.text('Code spaces…').last);
    await t.pumpAndSettle();
    expect(find.textContaining('alpha'), findsWidgets);
    await t.tap(find.textContaining('alpha').first);
    await t.pumpAndSettle();
    expect(find.text('★ alpha'), findsOneWidget);
  });

  testWidgets('Migrate banner offers a dry run', (t) async {
    final api = await pump(t, pending: 2);
    expect(find.byKey(const ValueKey('migrate-now')), findsOneWidget);
    await t.tap(find.byKey(const ValueKey('migrate-now')));
    await t.pumpAndSettle();
    expect(find.textContaining('Nothing to migrate'), findsOneWidget);
    await t.tap(find.text('Cancel'));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('migrate-later')));
    await t.pumpAndSettle();
    expect(api.migrateWasOffered, isTrue);
    expect(find.byKey(const ValueKey('migrate-now')), findsNothing);
  });
}
