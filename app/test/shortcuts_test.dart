import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:muninn/src/api.dart';
import 'package:muninn/src/app.dart';
import 'package:muninn/src/state.dart';
import 'package:muninn/src/views/chart.dart';

import 'fake_api.dart';

Future<(FakeApi, ProviderContainer)> pump(WidgetTester t, Size size) async {
  t.view.physicalSize = size;
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

Future<void> shiftLetter(WidgetTester t, LogicalKeyboardKey key) async {
  await t.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
  await t.sendKeyEvent(key);
  await t.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
  await t.pumpAndSettle();
}

void main() {
  testWidgets('? and F1 open Help; P/K/G navigate; i opens Import', (t) async {
    final (_, c) = await pump(t, const Size(1280, 800));

    await t.sendKeyEvent(LogicalKeyboardKey.f1);
    await t.pumpAndSettle();
    expect(c.read(sectionProvider), Section.help);
    expect(find.byKey(const ValueKey('help-title')), findsOneWidget);
    expect(find.textContaining('Flutter app'), findsWidgets);

    await shiftLetter(t, LogicalKeyboardKey.keyP);
    expect(c.read(sectionProvider), Section.projects);

    await shiftLetter(t, LogicalKeyboardKey.keyK);
    expect(c.read(sectionProvider), Section.net);

    await shiftLetter(t, LogicalKeyboardKey.keyG);
    expect(c.read(sectionProvider), Section.orgs);

    await t.sendKeyEvent(LogicalKeyboardKey.keyI);
    await t.pumpAndSettle();
    expect(find.text('Import'), findsWidgets);
    await t.tap(find.text('[X]'));
    await t.pumpAndSettle();

    await shiftLetter(t, LogicalKeyboardKey.slash);
    expect(c.read(sectionProvider), Section.help);
  });

  testWidgets('W opens Net peers and L opens chart layers', (t) async {
    final (_, c) = await pump(t, const Size(1280, 800));
    await shiftLetter(t, LogicalKeyboardKey.keyW);
    expect(c.read(sectionProvider), Section.net);

    await shiftLetter(t, LogicalKeyboardKey.keyL);
    expect(find.text('Chart layers'), findsOneWidget);
    expect(find.text('Candles'), findsOneWidget);
  });

  testWidgets('X clears the time range', (t) async {
    final (_, c) = await pump(t, const Size(1280, 800));
    c.read(timeRangeProvider.notifier).state = const TimeRange(
      from: 1704067200000,
    );
    await t.pumpAndSettle();
    await shiftLetter(t, LogicalKeyboardKey.keyX);
    expect(c.read(timeRangeProvider), TimeRange.all);
  });

  testWidgets('letter shortcuts are ignored while typing', (t) async {
    final (_, c) = await pump(t, const Size(1280, 800));
    await t.tap(find.byType(TextField));
    await t.pump();
    final before = c.read(sectionProvider);
    await t.sendKeyEvent(LogicalKeyboardKey.keyI);
    await t.pumpAndSettle();
    expect(find.text('Import'), findsNothing);
    expect(c.read(sectionProvider), before);

    await t.sendKeyEvent(LogicalKeyboardKey.f1);
    await t.pumpAndSettle();
    expect(c.read(sectionProvider), before);
  });

  testWidgets('chart layer keys, + zoom and Ctrl-w t match the TUI', (t) async {
    final (_, c) = await pump(t, const Size(1280, 800));
    c.read(sectionProvider.notifier).state = Section.data;
    await t.pumpAndSettle();
    await unlock(t);
    await t.tap(find.text('BTC/USD · 5m'));
    await t.pumpAndSettle();
    expect(find.byType(BarChart), findsOneWidget);
    expect(c.read(chartLayersProvider).volume, isTrue);

    await t.sendKeyEvent(LogicalKeyboardKey.keyV);
    await t.pumpAndSettle();
    expect(c.read(chartLayersProvider).volume, isFalse);

    await t.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await t.sendKeyEvent(LogicalKeyboardKey.digit3);
    await t.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await t.pumpAndSettle();
    expect(c.read(chartLayersProvider).grid, isFalse);

    expect(find.text('50 bars'), findsOneWidget);
    await t.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await t.sendKeyEvent(LogicalKeyboardKey.equal);
    await t.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await t.pumpAndSettle();
    expect(find.textContaining('of 50 bars'), findsOneWidget);

    await t.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await t.sendKeyEvent(LogicalKeyboardKey.keyW);
    await t.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await t.sendKeyEvent(LogicalKeyboardKey.keyT);
    await t.pumpAndSettle();
    expect(c.read(dataAsChartProvider), isFalse);
    expect(find.byType(BarTable), findsOneWidget);

    await t.sendKeyEvent(LogicalKeyboardKey.period);
    await t.pumpAndSettle();
    expect(c.read(selectedDatasetProvider), 'binance:ETH/USDT:3600');
  });

  testWidgets('F5 refresh does not throw and T still opens the picker', (
    t,
  ) async {
    await pump(t, const Size(1280, 800));
    await t.sendKeyEvent(LogicalKeyboardKey.f5);
    await t.pumpAndSettle();
    await shiftLetter(t, LogicalKeyboardKey.keyT);
    expect(find.text('Time range'), findsWidgets);
  });
}
