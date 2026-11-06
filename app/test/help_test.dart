import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
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

void main() {
  testWidgets('Help menu opens embedded topics, not a placeholder', (t) async {
    final (_, c) = await pump(t);
    await t.tap(find.text('Help'));
    await t.pumpAndSettle();
    await t.tap(find.text('Topics…'));
    await t.pumpAndSettle();
    expect(c.read(sectionProvider), Section.help);
    expect(find.textContaining('Coming to the app'), findsNothing);
    expect(find.byKey(const ValueKey('help-title')), findsOneWidget);
    expect(find.textContaining('Flutter app'), findsWidgets);
    expect(find.textContaining('same keys as the TUI'), findsOneWidget);
  });

  testWidgets('Help ▸ Keybindings opens the key reference', (t) async {
    await pump(t);
    await t.tap(find.text('Help'));
    await t.pumpAndSettle();
    await t.tap(find.text('Keybindings'));
    await t.pumpAndSettle();
    expect(find.text('Keybindings'), findsWidgets);
    expect(find.textContaining('F5'), findsWidgets);
    expect(find.textContaining('refresh'), findsWidgets);
  });

  testWidgets('search filters topics and a hit opens the page', (t) async {
    final (_, c) = await pump(t);
    c.read(sectionProvider.notifier).state = Section.help;
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('help-search')), 'keybind');
    await t.pumpAndSettle();
    expect(find.byKey(const ValueKey('help-hit-keybindings')), findsOneWidget);
    await t.tap(find.byKey(const ValueKey('help-hit-keybindings')));
    await t.pumpAndSettle();
    expect(c.read(helpTopicIdProvider), 'keybindings');
    expect(
      find.textContaining('Every key runs the same named action'),
      findsOneWidget,
    );
  });

  testWidgets('an internal link follows to that topic', (t) async {
    final (_, c) = await pump(t);
    c.read(helpTopicIdProvider.notifier).state = 'README';
    c.read(sectionProvider.notifier).state = Section.help;
    await t.pumpAndSettle();
    expect(find.text('Muninn'), findsWidgets);
    await t.tap(find.text('keybindings'));
    await t.pumpAndSettle();
    expect(c.read(helpTopicIdProvider), 'keybindings');
    await t.tap(find.byKey(const ValueKey('help-back')));
    await t.pumpAndSettle();
    expect(c.read(helpTopicIdProvider), 'README');
  });

  testWidgets('? opens contextual help for the current section', (t) async {
    final (_, c) = await pump(t);
    c.read(sectionProvider.notifier).state = Section.keys;
    await t.pumpAndSettle();
    await t.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await t.sendKeyEvent(LogicalKeyboardKey.slash);
    await t.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await t.pumpAndSettle();
    expect(c.read(sectionProvider), Section.help);
    expect(c.read(helpTopicIdProvider), 'keys');
    expect(find.text('Keys'), findsWidgets);
  });
}
