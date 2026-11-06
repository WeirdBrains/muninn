import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:muninn/src/api.dart';
import 'package:muninn/src/app.dart';
import 'package:muninn/src/state.dart';
import 'package:muninn/src/views/chart.dart';

import 'fake_api.dart';

void main() {
  test('sma and ema match the TUI', () {
    const v = [1.0, 2.0, 3.0, 4.0, 5.0];
    expect(sma(v, 3), [null, null, 2.0, 3.0, 4.0]);
    final e = ema(v, 3);
    expect(e[1], isNull);
    expect(e[4], greaterThan(3.5));
  });

  test('parseChartMas accepts the TUI prompt form', () {
    expect(parseChartMas('sma20, ema50 10').map((m) => m.label).toList(), [
      'sma20',
      'ema50',
      'sma10',
    ]);
    expect(() => parseChartMas('foo'), throwsFormatException);
    expect(() => parseChartMas('sma1'), throwsFormatException);
  });

  test('shootingStar matches the TUI rule', () {
    expect(
      shootingStar(
        const BarPoint(
          ts: 0,
          open: 10,
          high: 12,
          low: 9.9,
          close: 10.2,
          volume: 1,
        ),
      ),
      isTrue,
    );
    expect(
      shootingStar(
        const BarPoint(
          ts: 0,
          open: 10,
          high: 12.1,
          low: 9.9,
          close: 12,
          volume: 1,
        ),
      ),
      isFalse,
    );
  });

  testWidgets('chart layers sheet lists envelope, MAs and markers', (t) async {
    t.view.physicalSize = const Size(1280, 800);
    t.view.devicePixelRatio = 1;
    addTearDown(t.view.reset);
    final api = FakeApi();
    final c = ProviderContainer(
      overrides: [apiProvider.overrideWithValue(api)],
    );
    addTearDown(c.dispose);
    await t.pumpWidget(
      UncontrolledProviderScope(container: c, child: const MuninnApp()),
    );
    await t.pumpAndSettle();
    c.read(sectionProvider.notifier).state = Section.data;
    await t.pumpAndSettle();
    await t.enterText(find.byType(TextField), 'pw');
    await t.tap(find.text('Unlock'));
    await t.pumpAndSettle();
    await t.tap(find.text('BTC/USD · 5m'));
    await t.pumpAndSettle();

    await t.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await t.sendKeyEvent(LogicalKeyboardKey.keyL);
    await t.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await t.pumpAndSettle();
    expect(find.text('High/low envelope'), findsOneWidget);
    expect(find.text('Moving averages'), findsOneWidget);
    await t.tap(find.text('High/low envelope'));
    await t.pumpAndSettle();
    expect(c.read(chartLayersProvider).envelope, isTrue);

    final sheet = find.byType(Scrollable).last;
    await t.scrollUntilVisible(find.text('Wick markers'), 80, scrollable: sheet);
    expect(find.text('Wick markers'), findsOneWidget);
    await t.scrollUntilVisible(find.text('Gap markers'), 80, scrollable: sheet);
    expect(find.text('Gap markers'), findsOneWidget);
    await t.scrollUntilVisible(find.text('Zero-volume markers'), 80, scrollable: sheet);
    expect(find.text('Zero-volume markers'), findsOneWidget);
    await t.scrollUntilVisible(find.text('Moving averages…'), 80, scrollable: sheet);
    await t.tap(find.text('Moving averages…'));
    await t.pumpAndSettle();
    expect(find.byKey(const ValueKey('chart-mas')), findsOneWidget);
    await t.enterText(find.byKey(const ValueKey('chart-mas')), 'ema21, sma55');
    await t.tap(find.byKey(const ValueKey('chart-mas-save')));
    await t.pumpAndSettle();
    expect(c.read(chartMasProvider).map((m) => m.label).toList(), [
      'ema21',
      'sma55',
    ]);
  });
}
