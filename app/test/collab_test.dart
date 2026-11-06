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
  testWidgets('Rules menu creates a rule, not a placeholder', (t) async {
    final (api, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Projects'));
    await t.pumpAndSettle();
    await t.tap(find.text('Rules…'));
    await t.pumpAndSettle();
    expect(c.read(projectPaneProvider), ProjectPane.rules);
    expect(find.textContaining('Coming to the app'), findsNothing);
    await t.tap(find.byKey(const ValueKey('rule-new')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('rule-name')), 'imported');
    await t.enterText(find.byKey(const ValueKey('rule-on')), 'import');
    await t.enterText(find.byKey(const ValueKey('rule-then')), 'status hi');
    await t.tap(find.byKey(const ValueKey('rule-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('rule-create:imported:import'));
    expect(find.text('imported'), findsOneWidget);
  });

  testWidgets('Reviews menu requests a review, not a placeholder', (t) async {
    final (api, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Projects'));
    await t.pumpAndSettle();
    await t.ensureVisible(find.text('Reviews…'));
    await t.tap(find.text('Reviews…'));
    await t.pumpAndSettle();
    expect(c.read(projectPaneProvider), ProjectPane.reviews);
    expect(find.textContaining('Coming to the app'), findsNothing);
    await t.tap(find.byKey(const ValueKey('review-new')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('review-target')), 'notes');
    await t.tap(find.byKey(const ValueKey('review-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('review-request:journal:notes'));
    expect(find.textContaining('notes'), findsWidgets);
  });

  testWidgets('Notifications menu sends one, not a placeholder', (t) async {
    final (api, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Projects'));
    await t.pumpAndSettle();
    await t.ensureVisible(find.text('Notifications…'));
    await t.tap(find.text('Notifications…'));
    await t.pumpAndSettle();
    expect(c.read(projectPaneProvider), ProjectPane.notifications);
    expect(find.textContaining('Coming to the app'), findsNothing);
    expect(find.textContaining('No notifications yet'), findsOneWidget);
    await t.tap(find.byKey(const ValueKey('notify-send')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('notify-title')), 'hello');
    await t.tap(find.byKey(const ValueKey('notify-send-confirm')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('notify-send:hello'));
    expect(find.text('hello'), findsOneWidget);
  });
}
