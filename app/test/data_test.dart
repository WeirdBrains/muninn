import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:muninn/src/api.dart';
import 'package:muninn/src/app.dart';
import 'package:muninn/src/state.dart';
import 'package:muninn/src/time_range.dart';
import 'package:muninn/src/views/chart.dart';

import 'fake_api.dart';

Future<(FakeApi, ProviderContainer)> pump(WidgetTester t, Size size) async {
  t.view.physicalSize = size;
  t.view.devicePixelRatio = 1;
  addTearDown(t.view.reset);
  final api = FakeApi();
  final c = ProviderContainer(overrides: [apiProvider.overrideWithValue(api)]);
  addTearDown(c.dispose);
  await t.pumpWidget(UncontrolledProviderScope(container: c, child: const MuninnApp()));
  await t.pumpAndSettle();
  return (api, c);
}

Future<void> unlock(WidgetTester t) async {
  await t.enterText(find.byType(TextField), 'pw');
  await t.tap(find.text('Unlock'));
  await t.pumpAndSettle();
}

void main() {
  test('time range overlap matches the TUI sidebar', () {
    const d = Dataset(source: 's', base: 'B', quote: 'Q', duration: '5m', count: 1, first: 100, last: 200);
    expect(d.overlaps(TimeRange.all), isTrue);
    expect(d.overlaps(const TimeRange(from: 150)), isTrue);
    expect(d.overlaps(const TimeRange(from: 201)), isFalse);
    expect(d.overlaps(const TimeRange(to: 99)), isFalse);
    expect(d.overlaps(const TimeRange(from: 50, to: 100)), isTrue);
    expect(fmtUtc(1704067200000), '2024-01-01 00:00');
  });

  testWidgets('desktop: unlock, pick a dataset, chart and list', (t) async {
    final (api, c) = await pump(t, const Size(1280, 800));
    c.read(sectionProvider.notifier).state = Section.data;
    await t.pumpAndSettle();
    expect(find.text('Unlock'), findsOneWidget);
    await t.enterText(find.byType(TextField), 'nope');
    await t.tap(find.text('Unlock'));
    await t.pumpAndSettle();
    expect(find.textContaining('wrong key'), findsOneWidget);
    await unlock(t);
    expect(find.text('BTC/USD · 5m'), findsOneWidget);
    expect(find.text('Pick a dataset'), findsOneWidget);
    await t.tap(find.text('BTC/USD · 5m'));
    await t.pumpAndSettle();
    expect(find.byType(BarChart), findsOneWidget);
    expect(find.text('50 bars'), findsOneWidget);
    await t.tap(find.text('List'));
    await t.pumpAndSettle();
    expect(find.byType(BarTable), findsOneWidget);
  });

  testWidgets('time range scopes datasets and re-queries the chart', (t) async {
    final (api, c) = await pump(t, const Size(1280, 800));
    c.read(sectionProvider.notifier).state = Section.data;
    await t.pumpAndSettle();
    await unlock(t);
    await t.tap(find.text('BTC/USD · 5m'));
    await t.pumpAndSettle();
    c.read(timeRangeProvider.notifier).state = const TimeRange(from: 1704067200000);
    await t.pumpAndSettle();
    expect(find.text('ETH/USDT · 1h'), findsNothing, reason: '2021 data is outside the range');
    expect(find.text('from 2024-01-01 00:00 UTC'), findsOneWidget);
    expect(api.queries.last, const TimeRange(from: 1704067200000));
    await t.tap(find.byTooltip('Remove from'));
    await t.pumpAndSettle();
    expect(find.text('ETH/USDT · 1h'), findsOneWidget);
  });

  testWidgets('T and the Data menu open the picker, which has [X]', (t) async {
    await pump(t, const Size(1280, 800));
    await t.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await t.sendKeyEvent(LogicalKeyboardKey.keyT);
    await t.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await t.pumpAndSettle();
    expect(find.text('Time range'), findsWidgets);
    expect(find.text('From'), findsOneWidget);
    await t.tap(find.text('[X]'));
    await t.pumpAndSettle();
    expect(find.text('From'), findsNothing);
    await t.tap(find.text('Data'));
    await t.pumpAndSettle();
    await t.tap(find.text('Filters'));
    await t.pumpAndSettle();
    await t.tap(find.text('Time range…'));
    await t.pumpAndSettle();
    expect(find.text('From'), findsOneWidget);
    await t.tap(find.text('Apply'));
    await t.pumpAndSettle();
  });

  testWidgets('projects open their series in Data with the saved range', (t) async {
    final (_, c) = await pump(t, const Size(1280, 800));
    c.read(sectionProvider.notifier).state = Section.projects;
    await t.pumpAndSettle();
    await unlock(t);
    await t.tap(find.text('Alpha'));
    await t.pumpAndSettle();
    await t.tap(find.text('coinbase · BTC/USD'));
    await t.pumpAndSettle();
    expect(c.read(sectionProvider), Section.data);
    expect(c.read(timeRangeProvider), const TimeRange(from: 1704067200000));
    expect(find.byType(BarChart), findsOneWidget);
  });

  testWidgets('phone: data list opens the chart page', (t) async {
    final (_, c) = await pump(t, const Size(390, 844));
    await t.tap(find.text('Data'));
    await t.pumpAndSettle();
    await unlock(t);
    await t.tap(find.text('BTC/USD · 5m'));
    await t.pumpAndSettle();
    expect(find.byType(BarChart), findsOneWidget);
    expect(find.byType(BackButton), findsOneWidget);
    expect(c.read(selectedDatasetProvider), 'coinbase:BTC/USD:300');
  });
}
