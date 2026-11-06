import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:muninn/src/api.dart';
import 'package:muninn/src/app.dart';
import 'package:muninn/src/state.dart';
import 'package:muninn/src/tiling_extras.dart';
import 'package:muninn/src/workspace.dart';

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

Future<void> shiftKey(WidgetTester t, LogicalKeyboardKey key) async {
  await t.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
  await t.sendKeyEvent(key);
  await t.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
  await t.pumpAndSettle();
}

void main() {
  testWidgets('Y toggles the history bar; u and U step', (t) async {
    final (api, c) = await pump(t);
    c.read(sectionProvider.notifier).state = Section.data;
    await t.pumpAndSettle();
    await unlock(t);
    api.historyList = const [
      HistoryRowInfo(
        id: 1,
        kind: 'view.filter',
        summary: 'filter BTC',
        head: true,
        current: true,
      ),
    ];
    expect(find.byKey(const ValueKey('history-pane')), findsNothing);

    await shiftKey(t, LogicalKeyboardKey.keyY);
    expect(c.read(historyOpenProvider), isTrue);
    expect(find.byKey(const ValueKey('history-pane')), findsOneWidget);
    expect(find.text('filter BTC'), findsOneWidget);

    await t.sendKeyEvent(LogicalKeyboardKey.keyU);
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('history-back'));

    await shiftKey(t, LogicalKeyboardKey.keyU);
    expect(api.extraCalls, contains('history-forward'));

    await t.tap(find.byKey(const ValueKey('history-row-1')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('history-goto:1'));

    await shiftKey(t, LogicalKeyboardKey.keyY);
    expect(c.read(historyOpenProvider), isFalse);
    expect(find.byKey(const ValueKey('history-pane')), findsNothing);
    expect(api.extraCalls, contains('history-open:true'));
    expect(api.extraCalls, contains('history-open:false'));
  });

  testWidgets('File Authors adds and lists an author', (t) async {
    final (api, c) = await pump(t);
    await unlock(t);
    await shiftKey(t, LogicalKeyboardKey.keyA);
    expect(find.byKey(const ValueKey('authors-dialog')), findsOneWidget);

    await t.tap(find.byKey(const ValueKey('authors-add')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('author-name')), 'Ada');
    await t.enterText(
      find.byKey(const ValueKey('author-email')),
      'ada@example',
    );
    await t.enterText(
      find.byKey(const ValueKey('author-details')),
      'role=writer',
    );
    await t.tap(find.byKey(const ValueKey('author-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('author-add:Ada'));
    expect(find.text('Ada'), findsOneWidget);
    expect(find.textContaining('ada@example'), findsOneWidget);
  });

  testWidgets('View Layouts save and restore the current workspace', (t) async {
    final (api, c) = await pump(t);
    c.read(sectionProvider.notifier).state = Section.data;
    await t.pumpAndSettle();
    await unlock(t);

    var ws = c.read(workspaceProvider);
    ws = ws
        .split(SplitDir.horizontal, const Size(800, 600))
        .workspace
        .updateTile(
          1,
          (tile) => tile.copyWith(datasetKey: 'binance:BTC/USDT:300'),
        );
    c.read(workspaceProvider.notifier).state = ws;

    await t.tap(find.text('View'));
    await t.pumpAndSettle();
    await t.tap(find.text('Layouts'));
    await t.pumpAndSettle();
    await t.tap(find.text('Save current…'));
    await t.pumpAndSettle();
    expect(find.byKey(const ValueKey('layout-save-dialog')), findsOneWidget);
    await t.enterText(find.byKey(const ValueKey('layout-save-name')), 'desk');
    await t.tap(find.byKey(const ValueKey('layout-save-confirm')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('layout-save:desk'));
    expect(api.namedLayouts['desk'], contains('BTC/USDT'));

    c.read(workspaceProvider.notifier).state = Workspace.single();
    expect(c.read(workspaceProvider).tiles, hasLength(1));

    await t.tap(find.text('View'));
    await t.pumpAndSettle();
    await t.tap(find.text('Layouts'));
    await t.pumpAndSettle();
    await t.tap(find.text('Saved…'));
    await t.pumpAndSettle();
    expect(find.byKey(const ValueKey('layout-saved-dialog')), findsOneWidget);
    await t.tap(find.byKey(const ValueKey('layout-item-desk')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('layout-load:desk'));
    expect(c.read(workspaceProvider).tiles, hasLength(2));
    expect(
      c.read(workspaceProvider).tile(1)!.datasetKey,
      'binance:BTC/USDT:300',
    );
  });

  testWidgets('applying a journals search opens found-views', (t) async {
    final (api, c) = await pump(t);
    api.searchList = [
      const SearchInfo(
        id: 's1',
        shortId: 's1',
        name: 'Notes',
        target: 'journals',
        query: 'bitcoin',
      ),
    ];
    api.foundEntries = const [
      FoundEntryInfo(
        journalId: 'j1',
        journalName: 'Desk',
        entryId: 'e1',
        heading: 'BTC note',
        title: 'BTC note',
      ),
    ];
    api.journalList = const [
      JournalInfo(id: 'j1', shortId: 'j1', name: 'Desk'),
    ];
    c.read(sectionProvider.notifier).state = Section.projects;
    c.read(projectPaneProvider.notifier).state = ProjectPane.searches;
    await t.pumpAndSettle();
    await unlock(t);
    expect(find.text('Notes'), findsOneWidget);

    await t.tap(find.byKey(const ValueKey('search-menu-s1')));
    await t.pumpAndSettle();
    await t.tap(find.text('Apply'));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('found-entries:bitcoin'));
    expect(find.byKey(const ValueKey('found-entries-dialog')), findsOneWidget);
    expect(find.text('BTC note'), findsOneWidget);

    await t.tap(find.byKey(const ValueKey('found-entry-e1')));
    await t.pumpAndSettle();
    expect(c.read(sectionProvider), Section.projects);
    expect(c.read(projectPaneProvider), ProjectPane.journals);
    expect(c.read(selectedJournalProvider)?.id, 'j1');
  });

  test('linked list selection centres the paired chart', () {
    final ws = Workspace.single(datasetKey: 'a', asChart: false)
        .split(SplitDir.horizontal, const Size(800, 600), asChart: true)
        .workspace
        .focusTile(1)
        .cycleLink();
    expect(ws.tile(1)!.link, 2);
    expect(ws.linkedTo(1).single.id, 2);
    expect(ws.linkedTo(2).single.id, 1);
    expect(ws.tile(2)!.asChart, isTrue);
  });
}
