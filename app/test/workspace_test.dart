import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:muninn/src/app.dart';
import 'package:muninn/src/state.dart';
import 'package:muninn/src/workspace.dart';

import 'fake_api.dart';

const area = Size(800, 600);

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

Future<void> ctrlWThen(WidgetTester t, LogicalKeyboardKey key) async {
  await t.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
  await t.sendKeyEvent(LogicalKeyboardKey.keyW);
  await t.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
  await t.sendKeyEvent(key);
  await t.pumpAndSettle();
}

void main() {
  test('split copies the series, close refuses the last tile', () {
    var ws = Workspace.single(datasetKey: 'btc', asChart: true);
    expect(ws.tiles, hasLength(1));
    expect(ws.focused.asChart, isTrue);
    ws = ws.changeView();
    expect(ws.focused.asChart, isFalse);

    final split = ws.split(SplitDir.horizontal, area);
    expect(split.error, isNull);
    ws = split.workspace;
    expect(ws.tiles, hasLength(2));
    expect(ws.focus, 2);
    expect(ws.tile(2)!.asChart, isFalse);
    expect(ws.tile(2)!.datasetKey, 'btc');
    expect(ws.tile(1)!.datasetKey, 'btc');

    ws = ws.focusPrev();
    expect(ws.focus, 1);
    ws = ws.focusNext();
    expect(ws.focus, 2);
    ws = ws.focusTile(1);
    expect(ws.focus, 1);

    final closed = ws.closeTile();
    expect(closed.error, isNull);
    ws = closed.workspace;
    expect(ws.tiles, hasLength(1));
    expect(ws.closeTile().error, "can't close the last tile");
  });

  test('refuses a split below the minimum tile size', () {
    final ws = Workspace.single();
    final op = ws.split(SplitDir.horizontal, const Size(300, 400));
    expect(op.error, contains('too small'));
    expect(op.workspace.tiles, hasLength(1));
  });

  test('zoom, resize, equalize, swap, link and sidebar', () {
    var ws = Workspace.single(datasetKey: 'a')
        .split(SplitDir.horizontal, area)
        .workspace;
    expect(ws.focus, 2);

    final grown = ws.resize(FocusDir.right, area);
    expect(grown.error, isNull);
    expect((grown.workspace.root as WsSplit).ratio, closeTo(0.45, 0.001));
    ws = grown.workspace.equalize();
    expect((ws.root as WsSplit).ratio, closeTo(0.5, 0.001));

    ws = ws.toggleZoom();
    expect(ws.zoomed, isTrue);
    expect(ws.rects(area), hasLength(1));
    expect(ws.rects(area).single.id, ws.focus);
    ws = ws.toggleZoom();
    expect(ws.zoomed, isFalse);
    expect(ws.rects(area), hasLength(2));

    final swapped = ws.swap();
    expect(swapped.error, isNull);
    ws = swapped.workspace;
    expect(ws.tiles.map((t) => t.id).toList(), [2, 1]);

    ws = ws.focusTile(2).cycleLink();
    expect(ws.focused.link, 1);
    ws = ws.cycleLink();
    expect(ws.focused.link, isNull);

    expect(ws.sidebarVisible, isTrue);
    ws = ws.toggleSidebar();
    expect(ws.sidebarVisible, isFalse);
    expect(ws.sidebarFocused, isFalse);
    ws = ws.focusSidebar();
    expect(ws.sidebarVisible, isTrue);
    expect(ws.sidebarFocused, isTrue);
    ws = ws.unfocusSidebar();
    expect(ws.sidebarFocused, isFalse);
  });

  test('presets and container modes match the TUI shapes', () {
    var ws = Workspace.single(
      datasetKey: 's',
      asChart: true,
    ).preset(LayoutPreset.twoUp);
    expect(ws.tiles, hasLength(2));
    expect(ws.tile(1)!.asChart, isFalse);
    expect(ws.tile(2)!.asChart, isTrue);
    expect(ws.tile(1)!.datasetKey, 's');
    expect(ws.focus, 1);

    ws = ws.preset(LayoutPreset.threeColumn);
    expect(ws.tiles, hasLength(3));
    ws = ws.preset(LayoutPreset.mainStack);
    expect(ws.tiles, hasLength(3));
    ws = ws.preset(LayoutPreset.grid2x2);
    expect(ws.tiles, hasLength(4));

    ws = Workspace.single().split(SplitDir.vertical, area).workspace;
    final boxed = ws.setContainer(ContainerMode.tabbed);
    expect(boxed.error, isNull);
    expect((boxed.workspace.root as WsSplit).mode, ContainerMode.tabbed);
    expect(
      Workspace.single().setContainer(ContainerMode.stacked).error,
      contains('no container'),
    );
  });

  test('named-layout JSON round-trips layers, viewport and links', () {
    var ws = Workspace.single(datasetKey: 'binance:BTC/USDT:300')
        .split(SplitDir.horizontal, area)
        .workspace;
    ws = ws
        .focusTile(1)
        .updateTile(
          1,
          (t) => t.copyWith(
            layers: t.layers.copyWith(envelope: true, volume: false),
            viewport: const ChartViewport(scale: 2, start: 3),
          ),
        )
        .focusTile(2)
        .cycleLink();
    expect(ws.tile(2)!.link, 1);
    expect(ws.linkedTo(1).map((t) => t.id), [2]);
    expect(ws.linkedTo(2).map((t) => t.id), [1]);

    final json = ws.encode();
    expect(json, contains('binance:BTC/USDT:300'));
    expect(json, contains('"envelope":true'));
    final loaded = Workspace.decode(json);
    expect(loaded.focus, ws.focus);
    expect(loaded.tiles, hasLength(2));
    expect(loaded.tile(1)!.layers.envelope, isTrue);
    expect(loaded.tile(1)!.layers.volume, isFalse);
    expect(loaded.tile(1)!.viewport.scale, 2);
    expect(loaded.tile(1)!.viewport.start, 3);
    expect(loaded.tile(2)!.link, 1);
    expect(loaded.tile(2)!.asChart, isTrue);
    expect(loaded.linkedTo(1).single.id, 2);
  });

  test('named-layout JSON round-trips a pipeline monitor frame', () {
    final ws = Workspace.single().openMonitor('feed', area);
    expect(ws.focused.frameKey, 'monitor:feed');
    expect(ws.focused.isMonitor, isTrue);
    expect(ws.focused.monitorPipeline, 'feed');
    expect(ws.encode(), contains('monitor:feed'));
    final loaded = Workspace.decode(ws.encode());
    expect(loaded.focused.frameKey, 'monitor:feed');
    expect(loaded.focused.isMonitor, isTrue);
    expect(loaded.focused.datasetKey, isNull);
  });

  testWidgets(
    'Ctrl-w v splits; Tab and Alt-2 move focus; Ctrl-b hides the sidebar',
    (t) async {
      final (_, c) = await pump(t, const Size(1280, 800));
      c.read(sectionProvider.notifier).state = Section.data;
      await t.pumpAndSettle();
      await unlock(t);
      await t.tap(find.text('BTC/USD · 5m'));
      await t.pumpAndSettle();
      expect(find.byKey(const ValueKey('data-tile-1')), findsOneWidget);

      await ctrlWThen(t, LogicalKeyboardKey.keyV);
      expect(c.read(workspaceProvider).tiles, hasLength(2));
      expect(c.read(workspaceProvider).focus, 2);
      expect(find.byKey(const ValueKey('data-tile-1')), findsOneWidget);
      expect(find.byKey(const ValueKey('data-tile-2')), findsOneWidget);

      await t.sendKeyEvent(LogicalKeyboardKey.tab);
      await t.pumpAndSettle();
      expect(c.read(workspaceProvider).focus, 1);

      await t.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
      await t.sendKeyEvent(LogicalKeyboardKey.digit2);
      await t.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
      await t.pumpAndSettle();
      expect(c.read(workspaceProvider).focus, 2);

      await t.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      await t.sendKeyEvent(LogicalKeyboardKey.keyB);
      await t.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await t.pumpAndSettle();
      expect(c.read(workspaceProvider).sidebarVisible, isFalse);
      expect(find.text('BTC/USD · 5m'), findsNothing);
    },
  );

  testWidgets('View menu lists TUI tile actions and not saved layouts', (
    t,
  ) async {
    await pump(t, const Size(1280, 800));
    await t.tap(find.text('View'));
    await t.pumpAndSettle();
    expect(find.text('New tile'), findsOneWidget);
    expect(find.text('Close tile'), findsOneWidget);
    expect(find.text('Zoom tile'), findsOneWidget);
    expect(find.text('Equalize'), findsOneWidget);
    expect(find.text('Change view'), findsOneWidget);
    expect(find.text('Link tile'), findsOneWidget);
    expect(find.text('Toggle sidebar'), findsOneWidget);
    expect(find.text('Saved…'), findsNothing);
    expect(find.text('Save current…'), findsNothing);
  });

  testWidgets('Shift+J opens Journals', (t) async {
    final (_, c) = await pump(t, const Size(1280, 800));
    await t.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await t.sendKeyEvent(LogicalKeyboardKey.keyJ);
    await t.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await t.pumpAndSettle();
    expect(c.read(sectionProvider), Section.projects);
    expect(c.read(projectPaneProvider), ProjectPane.journals);
  });

  testWidgets('phone width does not tile or hide the list with Ctrl-b', (
    t,
  ) async {
    final (_, c) = await pump(t, const Size(400, 800));
    c.read(sectionProvider.notifier).state = Section.data;
    await t.pumpAndSettle();
    await unlock(t);
    expect(find.byKey(const ValueKey('data-tile-1')), findsNothing);
    expect(c.read(workspaceProvider).tiles, hasLength(1));
    await ctrlWThen(t, LogicalKeyboardKey.keyV);
    expect(c.read(workspaceProvider).tiles, hasLength(1));
    await t.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await t.sendKeyEvent(LogicalKeyboardKey.keyB);
    await t.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await t.pumpAndSettle();
    expect(c.read(workspaceProvider).sidebarVisible, isTrue);
    expect(find.text('BTC/USD · 5m'), findsOneWidget);
  });
}
