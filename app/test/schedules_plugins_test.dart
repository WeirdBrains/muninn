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
  final api = FakeApi()
    ..pluginList = [
      const PluginInfo(
        name: 'csv',
        kind: 'parser',
        lang: 'math',
        main: 'csv-main',
        approved: true,
      ),
      const PluginInfo(
        name: 'binance',
        kind: 'ingest-connector',
        lang: 'python',
        main: 'binance-main',
        approved: true,
      ),
    ]
    ..rulesetList = [
      const RulesetInfo(id: 'r1', name: 'Cleanup', rules: ['tidy']),
    ]
    ..scheduleList = [
      const ScheduleInfo(
        id: 's1',
        name: 'nightly',
        targetKind: 'ruleset',
        target: 'Cleanup',
        runEvery: '1 day',
        describe: ['every 1 day'],
      ),
    ];
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
  testWidgets('Plugins menu lists by kind', (t) async {
    final (_, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Plug'));
    await t.pumpAndSettle();
    await t.ensureVisible(find.text('Parsers…'));
    await t.tap(find.text('Parsers…'));
    await t.pumpAndSettle();
    expect(c.read(projectPaneProvider), ProjectPane.plugins);
    expect(c.read(pluginKindFilterProvider), 'parser');
    expect(find.text('csv'), findsOneWidget);
    expect(find.text('binance'), findsNothing);
  });

  testWidgets('Schedules menu opens form with Name/Target/Start/Repeat', (
    t,
  ) async {
    final (api, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Projects'));
    await t.pumpAndSettle();
    await t.ensureVisible(find.text('Schedules…'));
    await t.tap(find.text('Schedules…'));
    await t.pumpAndSettle();
    expect(c.read(projectPaneProvider), ProjectPane.schedules);
    expect(find.text('nightly'), findsOneWidget);
    await t.tap(find.text('New schedule…').first);
    await t.pumpAndSettle();
    expect(find.byKey(const ValueKey('schedule-name')), findsOneWidget);
    expect(find.byKey(const ValueKey('schedule-target')), findsOneWidget);
    expect(find.byKey(const ValueKey('schedule-start')), findsOneWidget);
    expect(find.byKey(const ValueKey('schedule-mode')), findsOneWidget);
    await t.enterText(find.byKey(const ValueKey('schedule-name')), 'hourly');
    await t.enterText(find.byKey(const ValueKey('schedule-target')), 'Cleanup');
    await t.enterText(find.byKey(const ValueKey('schedule-every')), '1h');
    await t.tap(find.byKey(const ValueKey('schedule-save')));
    await t.pumpAndSettle();
    expect(api.scheduleList.any((s) => s.name == 'hourly'), isTrue);
  });

  testWidgets('Rule sets menu lists sets', (t) async {
    final (_, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Projects'));
    await t.pumpAndSettle();
    await t.ensureVisible(find.text('Rule sets…'));
    await t.tap(find.text('Rule sets…'));
    await t.pumpAndSettle();
    expect(c.read(projectPaneProvider), ProjectPane.rulesets);
    expect(find.textContaining('Cleanup'), findsWidgets);
  });

  testWidgets('shareCreateThen fake covers create-to', (t) async {
    final api = FakeApi();
    final out = await api.shareCreateThen(
      kind: 'journal',
      item: 'Notes',
      target: 'project',
      createItem: true,
      createTo: 'Alpha',
    );
    expect(out.newShare, isTrue);
    expect(out.createdToName, 'Alpha');
  });
}
