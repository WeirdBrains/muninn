import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:muninn/src/api.dart';
import 'package:muninn/src/app.dart';
import 'package:muninn/src/state.dart';
import 'package:muninn/src/time_range.dart';

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

void main() {
  test('parseUtcMs reads the TUI prompt formats', () {
    expect(parseUtcMs('2024-01-01'), 1704067200000);
    expect(parseUtcMs('2024-01-01 01:00'), 1704070800000);
    expect(parseUtcMs('2024-01-01T01:00:00'), 1704070800000);
    expect(parseUtcMs('1704067200'), 1704067200000);
    expect(parseUtcMs('not a date'), isNull);
  });

  test('ChartViewport period start/end/jump match the TUI window math', () {
    const v = ChartViewport();
    const n = 50;
    final start = v.periodStart(12, n);
    expect(start.start, 12);
    expect(start.visibleCount(n), 38);
    expect(start.periodStart(49, n), start, reason: 'start must be before end');

    final end = v.periodEnd(24, n);
    expect(end.start, 0);
    expect(end.visibleCount(n), 25);

    final jumped = ChartViewport(scale: 2, start: 0).jumpTo(30, n);
    expect(jumped.visibleStart(n) <= 30, isTrue);
    expect(jumped.visibleStart(n) + jumped.visibleCount(n) > 30, isTrue);
    expect(const ChartViewport().jumpTo(30, n).scale, 1);
  });

  test('nearestBarIndex picks the closest timestamp', () {
    final bars = [
      for (var i = 0; i < 10; i++)
        BarPoint(
          ts: 1704067200000 + i * 300000,
          open: 1,
          high: 1,
          low: 1,
          close: 1,
          volume: 1,
        ),
    ];
    expect(nearestBarIndex(bars, 1704067200000), 0);
    expect(nearestBarIndex(bars, 1704067200000 + 12 * 30000), 1);
  });

  testWidgets('Data Sources lists, adds, renames, removes and shows stored data', (
    t,
  ) async {
    final (api, _) = await pump(t);
    api.sourceData['coinbase'] = const [
      SourceDataItem(
        id: 'abcdef12ffff',
        name: 'ticks.csv',
        format: 'csv',
        locator: '/d/ticks.csv',
        size: 12,
        receivedAt: 1704067200000,
      ),
    ];
    api.sources = [
      const SourceInfo(
        id: 's-cb',
        name: 'coinbase',
        kind: 'csv',
        items: 1,
        bytes: 12,
      ),
      const SourceInfo(id: 's-bn', name: 'binance', kind: 'csv'),
    ];

    await t.tap(find.text('Data').first);
    await t.pumpAndSettle();
    await t.tap(find.text('Sources…'));
    await t.pumpAndSettle();
    expect(find.byKey(const ValueKey<String>('sources-dialog')), findsOneWidget);
    expect(find.text('coinbase'), findsOneWidget);
    expect(find.textContaining('1 item(s), 12 bytes'), findsOneWidget);

    await t.tap(find.byKey(const ValueKey<String>('source-add')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey<String>('source-name')), 'local');
    await t.enterText(
      find.byKey(const ValueKey<String>('source-kind')),
      'directory',
    );
    await t.enterText(
      find.byKey(const ValueKey<String>('source-locator')),
      '/d',
    );
    await t.tap(find.byKey(const ValueKey<String>('source-add-confirm')));
    await t.pumpAndSettle();
    expect(api.catalogCalls, contains('addSource local directory /d'));
    expect(find.text('local'), findsOneWidget);

    await t.tap(find.byKey(const ValueKey<String>('source-coinbase')));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey<String>('source-rename')));
    await t.pumpAndSettle();
    await t.enterText(
      find.byKey(const ValueKey<String>('source-rename-to')),
      'cb',
    );
    await t.tap(find.byKey(const ValueKey<String>('source-rename-confirm')));
    await t.pumpAndSettle();
    expect(api.catalogCalls, contains('renameSource coinbase cb'));
    expect(find.text('cb'), findsOneWidget);
    expect(find.text('coinbase'), findsNothing);

    await t.tap(find.byKey(const ValueKey<String>('source-cb')));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey<String>('source-data')));
    await t.pumpAndSettle();
    expect(find.byKey(const ValueKey<String>('source-data-dialog')), findsOneWidget);
    expect(find.text('ticks.csv'), findsOneWidget);
    expect(find.textContaining('abcdef12'), findsOneWidget);
    await t.tap(find.text('Close'));
    await t.pumpAndSettle();

    await t.tap(find.byKey(const ValueKey<String>('source-remove')));
    await t.pumpAndSettle();
    expect(find.textContaining('remove source `cb` and all of its bars?'), findsOneWidget);
    await t.tap(find.byKey(const ValueKey<String>('source-remove-confirm')));
    await t.pumpAndSettle();
    expect(api.catalogCalls, contains('removeSource cb'));
    expect(find.text('cb'), findsNothing);
  });

  testWidgets('Data Symbols lists pairs and adds a symbol with an alias', (
    t,
  ) async {
    final (api, _) = await pump(t);
    await t.tap(find.text('Data').first);
    await t.pumpAndSettle();
    await t.tap(find.text('Symbols…'));
    await t.pumpAndSettle();
    expect(find.byKey(const ValueKey<String>('symbols-dialog')), findsOneWidget);
    expect(find.text('BTC/USD'), findsWidgets);

    await t.tap(find.byKey(const ValueKey<String>('symbol-add')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey<String>('symbol-name')), 'sol-usd');
    await t.enterText(find.byKey(const ValueKey<String>('symbol-aliases')), 'SOLANA');
    await t.tap(find.byKey(const ValueKey<String>('symbol-add-confirm')));
    await t.pumpAndSettle();
    expect(api.catalogCalls.any((c) => c.startsWith('addSymbol SOL-USD') || c.startsWith('addSymbol sol-usd')), isTrue);
    expect(find.text('SOL/USD'), findsOneWidget);
    expect(find.text('alias SOLANA -> SOL'), findsOneWidget);
  });
}
