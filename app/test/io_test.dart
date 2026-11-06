import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:muninn/src/api.dart';
import 'package:muninn/src/app.dart';
import 'package:muninn/src/state.dart';

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
  testWidgets('File Import opens a path dialog, not a placeholder', (t) async {
    await pump(t, const Size(1280, 800));
    await t.tap(find.text('File'));
    await t.pumpAndSettle();
    await t.tap(find.text('Import…'));
    await t.pumpAndSettle();
    expect(find.textContaining('Not in the app yet'), findsNothing);
    expect(find.byKey(const ValueKey('import-path')), findsOneWidget);
    expect(find.textContaining('Stores each file as it is'), findsOneWidget);
  });

  testWidgets('File Export opens csv/parquet/json, not a placeholder', (t) async {
    await pump(t, const Size(1280, 800));
    await t.tap(find.text('File'));
    await t.pumpAndSettle();
    await t.tap(find.text('Export…'));
    await t.pumpAndSettle();
    expect(find.textContaining('Not in the app yet'), findsNothing);
    expect(find.text('CSV'), findsOneWidget);
    expect(find.text('Parquet'), findsOneWidget);
    expect(find.text('JSON'), findsOneWidget);
    expect(find.byKey(const ValueKey('export-path')), findsOneWidget);
  });

  testWidgets('import calls ingestPath and shows the summary', (t) async {
    final (api, _) = await pump(t, const Size(1280, 800));
    await unlock(t);
    await t.tap(find.text('File'));
    await t.pumpAndSettle();
    await t.tap(find.text('Import…'));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('import-path')), '/tmp/math');
    await t.enterText(find.byKey(const ValueKey('import-source')), 'given');
    await t.tap(find.byKey(const ValueKey('import-start')));
    await t.pumpAndSettle();
    expect(api.ingests, hasLength(1));
    expect(api.ingests.single.path, '/tmp/math');
    expect(api.ingests.single.source, 'given');
    expect(find.textContaining('Stored 1 file'), findsOneWidget);
  });

  testWidgets('export calls exportBars with the selected format and path', (t) async {
    final (api, c) = await pump(t, const Size(1280, 800));
    await unlock(t);
    c.read(selectedDatasetProvider.notifier).state = 'coinbase:BTC/USD:300';
    c.read(timeRangeProvider.notifier).state = const TimeRange(from: 1704067200000);
    await t.pumpAndSettle();
    await t.tap(find.text('File'));
    await t.pumpAndSettle();
    await t.tap(find.text('Export…'));
    await t.pumpAndSettle();
    await t.tap(find.text('JSON'));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('export-path')), '/tmp/out');
    await t.tap(find.byKey(const ValueKey('export-start')));
    await t.pumpAndSettle();
    expect(api.exports, hasLength(1));
    expect(api.exports.single.path, '/tmp/out.json');
    expect(api.exports.single.format, 'json');
    expect(api.exports.single.source, 'coinbase');
    expect(api.exports.single.symbol, 'BTC/USD');
    expect(api.exports.single.duration, '5m');
    expect(api.exports.single.from, '1704067200000');
    expect(find.textContaining('Wrote 3 bars'), findsOneWidget);
  });

  testWidgets('import without a path asks for one', (t) async {
    await pump(t, const Size(1280, 800));
    await t.tap(find.text('File'));
    await t.pumpAndSettle();
    await t.tap(find.text('Import…'));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('import-start')));
    await t.pumpAndSettle();
    expect(find.text('Choose a file or folder.'), findsOneWidget);
  });
}
