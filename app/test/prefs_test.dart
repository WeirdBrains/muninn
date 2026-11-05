import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:muninn/src/api.dart';
import 'package:muninn/src/app.dart';
import 'package:muninn/src/state.dart';

import 'fake_api.dart';

Future<ProviderContainer> pumpWith(WidgetTester t, FakeApi api) async {
  t.view.physicalSize = const Size(1280, 800);
  t.view.devicePixelRatio = 1;
  addTearDown(t.view.reset);
  final c = ProviderContainer(overrides: [apiProvider.overrideWithValue(api)]);
  addTearDown(c.dispose);
  await t.pumpWidget(UncontrolledProviderScope(container: c, child: const MuninnApp()));
  await t.pumpAndSettle();
  return c;
}

void main() {
  testWidgets('restores the saved theme, range and dataset without writing them back', (t) async {
    final api = FakeApi()
      ..saved = const SavedState(theme: 'nord', mode: 'light', range: TimeRange(from: 1704067200000), dataset: 'coinbase:BTC/USD:300');
    final c = await pumpWith(t, api);
    expect(c.read(themeSelectionProvider).name, 'nord');
    expect(c.read(themeSelectionProvider).mode, 'light');
    expect(c.read(timeRangeProvider), const TimeRange(from: 1704067200000));
    expect(c.read(selectedDatasetProvider), 'coinbase:BTC/USD:300');
    expect(api.saves, isEmpty);
  });

  testWidgets('changes are saved to preferences', (t) async {
    final api = FakeApi();
    final c = await pumpWith(t, api);
    c.read(themeSelectionProvider.notifier).set('nord');
    c.read(themeSelectionProvider.notifier).setMode('light');
    c.read(timeRangeProvider.notifier).state = const TimeRange(from: 1, to: 2);
    c.read(selectedDatasetProvider.notifier).state = 'binance:ETH/USDT:3600';
    await t.pumpAndSettle();
    expect(api.saves, ['theme=nord', 'mode=light', 'range=1..2', 'dataset=binance:ETH/USDT:3600']);
  });
}
