import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:muninn/src/app.dart';
import 'package:muninn/src/api.dart';
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

Future<void> unlock(WidgetTester t) async {
  await t.enterText(find.byType(TextField), 'pw');
  await t.tap(find.text('Unlock'));
  await t.pumpAndSettle();
}

Future<void> openProjects(WidgetTester t, ProviderContainer c) async {
  c.read(sectionProvider.notifier).state = Section.projects;
  await t.pumpAndSettle();
  await unlock(t);
}

void main() {
  testWidgets('journals pane creates, writes, notes and deletes', (t) async {
    final (api, c) = await pump(t);
    await openProjects(t, c);
    await t.tap(find.byKey(const ValueKey('projects-pane-journals')));
    await t.pumpAndSettle();
    expect(find.text('No journals yet'), findsOneWidget);

    await t.tap(find.byKey(const ValueKey('journals-new')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('journal-name')), 'Desk notes');
    await t.tap(find.byKey(const ValueKey('journal-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('journal-create:Desk notes'));
    expect(find.text('Desk notes'), findsOneWidget);

    await t.tap(find.byKey(const ValueKey('journal-add-entry')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('entry-title')), 'Monday');
    await t.enterText(find.byKey(const ValueKey('entry-body')), 'hello');
    await t.tap(find.byKey(const ValueKey('entry-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('entry-add:j1'));
    expect(find.text('Monday'), findsOneWidget);

    await t.tap(find.byKey(const ValueKey('journal-add-note')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('note-body')), 'a note');
    await t.tap(find.byKey(const ValueKey('note-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('note:j1'));
    expect(find.text('a note'), findsOneWidget);

    await t.tap(find.byKey(const ValueKey('journal-menu-j1')));
    await t.pumpAndSettle();
    await t.tap(find.text('Delete…'));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('journal-delete-confirm')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('journal-remove:j1'));
    expect(find.text('No journals yet'), findsOneWidget);
  });

  testWidgets('searches pane saves, renames and forks', (t) async {
    final (api, c) = await pump(t);
    await openProjects(t, c);
    await t.tap(find.byKey(const ValueKey('projects-pane-searches')));
    await t.pumpAndSettle();
    expect(find.text('No live searches yet'), findsOneWidget);

    await t.tap(find.byKey(const ValueKey('searches-new')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('search-name')), 'BTC hunt');
    await t.enterText(find.byKey(const ValueKey('search-query')), 'BTC');
    await t.tap(find.byKey(const ValueKey('search-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('search-save:BTC hunt'));
    expect(find.text('BTC hunt'), findsOneWidget);

    await t.tap(find.byKey(const ValueKey('search-menu-s1')));
    await t.pumpAndSettle();
    await t.tap(find.text('Rename…'));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('search-name')), 'ETH hunt');
    await t.tap(find.byKey(const ValueKey('search-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('search-rename:s1'));
    expect(find.text('ETH hunt'), findsOneWidget);

    await t.tap(find.byKey(const ValueKey('search-menu-s1')));
    await t.pumpAndSettle();
    await t.tap(find.text('Fork…'));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('search-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('search-fork:s1'));
    expect(find.text('ETH hunt (fork)'), findsOneWidget);

    await t.tap(find.byKey(const ValueKey('search-menu-s1')));
    await t.pumpAndSettle();
    await t.tap(find.text('Refine…'));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('search-sources')), 'binance');
    await t.enterText(find.byKey(const ValueKey('search-symbols')), 'ETH/USDT');
    await t.enterText(find.byKey(const ValueKey('search-periods')), '1h');
    await t.enterText(find.byKey(const ValueKey('search-from')), '2024-01-01');
    await t.enterText(find.byKey(const ValueKey('search-to')), '2024-01-02');
    await t.tap(find.byKey(const ValueKey('search-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('search-edit:s1'));
    expect(api.searchList.first.sources, ['binance']);
    expect(api.searchList.first.from, isNotNull);

    await t.tap(find.byKey(const ValueKey('search-menu-s1')));
    await t.pumpAndSettle();
    await t.tap(find.text('Apply to Data'));
    await t.pumpAndSettle();
    expect(c.read(sectionProvider), Section.data);
    expect(c.read(timeRangeProvider).from, isNotNull);
    expect(c.read(selectedDatasetProvider), 'binance:ETH/USDT:3600');

    c.read(sectionProvider.notifier).state = Section.projects;
    c.read(projectPaneProvider.notifier).state = ProjectPane.searches;
    await t.pumpAndSettle();
    c.read(timeRangeProvider.notifier).state = const TimeRange(
      from: 1000,
      to: 2000,
    );
    c.read(selectedDatasetProvider.notifier).state = 'coinbase:BTC/USD:300';
    await t.tap(find.byKey(const ValueKey('search-menu-s1')));
    await t.pumpAndSettle();
    await t.tap(find.text('Overwrite with current view'));
    await t.pumpAndSettle();
    expect(api.searchList.first.sources, ['coinbase']);
    expect(api.searchList.first.symbols, ['BTC/USD']);
    expect(api.searchList.first.periods, ['5m']);
    expect(api.searchList.first.from, 1000);
    expect(api.searchList.first.to, 2000);
  });

  testWidgets('reports pane creates a text section and deletes', (t) async {
    final (api, c) = await pump(t);
    await openProjects(t, c);
    await t.tap(find.byKey(const ValueKey('projects-pane-reports')));
    await t.pumpAndSettle();
    expect(find.text('No reports yet'), findsOneWidget);

    await t.tap(find.byKey(const ValueKey('reports-new')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('report-name')), 'Weekly');
    await t.enterText(
      find.byKey(const ValueKey('report-description')),
      'notes',
    );
    await t.tap(find.byKey(const ValueKey('report-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('report-create:Weekly'));
    expect(find.text('Weekly'), findsOneWidget);

    await t.tap(find.byKey(const ValueKey('report-add-section')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('section-title')), 'Intro');
    await t.enterText(find.byKey(const ValueKey('section-body')), 'hello');
    await t.tap(find.byKey(const ValueKey('section-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('section-add:r1:text'));
    expect(find.text('Intro'), findsOneWidget);

    await t.tap(find.byKey(const ValueKey('report-add-section')));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('section-kind')));
    await t.pumpAndSettle();
    await t.tap(find.text('records').last);
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('section-title')), 'Bars');
    await t.enterText(
      find.byKey(const ValueKey('section-target')),
      'coinbase:BTC/USD:5m',
    );
    await t.tap(find.byKey(const ValueKey('section-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('section-add:r1:records'));
    expect(find.text('Bars'), findsOneWidget);

    await t.tap(find.byKey(const ValueKey('report-preview')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('report-preview:r1'));
    expect(find.byKey(const ValueKey('report-preview-text')), findsOneWidget);
    await t.tap(find.text('Close'));
    await t.pumpAndSettle();

    await t.tap(find.byKey(const ValueKey('report-export')));
    await t.pumpAndSettle();
    await t.enterText(
      find.byKey(const ValueKey('report-export-out')),
      '/tmp/weekly.pdf',
    );
    await t.tap(find.byKey(const ValueKey('report-export-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('report-export:r1:pdf:/tmp/weekly.pdf'));

    await t.tap(find.byKey(const ValueKey('report-menu-r1')));
    await t.pumpAndSettle();
    await t.tap(find.text('Delete…'));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('report-delete-confirm')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('report-remove:r1'));
    expect(find.text('No reports yet'), findsOneWidget);
  });

  testWidgets('import archive and add a journal item to a project', (t) async {
    final (api, c) = await pump(t);
    await openProjects(t, c);
    await t.tap(find.byKey(const ValueKey('projects-import')));
    await t.pumpAndSettle();
    await t.enterText(
      find.byKey(const ValueKey('project-import-path')),
      '/tmp/pack.json',
    );
    await t.enterText(
      find.byKey(const ValueKey('project-import-name')),
      'Pack',
    );
    await t.tap(find.byKey(const ValueKey('project-import-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('import:/tmp/pack.json'));
    expect(find.text('Pack'), findsOneWidget);

    await t.tap(find.text('Alpha'));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('project-add-item')));
    await t.pumpAndSettle();
    await t.enterText(
      find.byKey(const ValueKey('project-add-key')),
      'Desk notes',
    );
    await t.tap(find.byKey(const ValueKey('project-add-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('add-item:p1:journal:Desk notes'));
    expect(find.text('Desk notes'), findsOneWidget);

    await t.tap(find.byKey(const ValueKey('project-export')));
    await t.pumpAndSettle();
    await t.enterText(
      find.byKey(const ValueKey('project-export-path')),
      '/tmp/alpha',
    );
    await t.tap(find.byKey(const ValueKey('project-export-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('export:p1:/tmp/alpha:archive'));
  });

  testWidgets('Projects menu opens journals and save-search dialogs', (
    t,
  ) async {
    final (_, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Projects').first);
    await t.pumpAndSettle();
    await t.tap(find.text('New journal…'));
    await t.pumpAndSettle();
    expect(c.read(sectionProvider), Section.projects);
    expect(c.read(projectPaneProvider), ProjectPane.journals);
    expect(find.byKey(const ValueKey('journal-name')), findsOneWidget);
    await t.tap(find.text('Cancel'));
    await t.pumpAndSettle();

    await t.tap(find.text('Projects').first);
    await t.pumpAndSettle();
    await t.tap(find.text('Save search as live search…'));
    await t.pumpAndSettle();
    expect(c.read(projectPaneProvider), ProjectPane.searches);
    expect(find.byKey(const ValueKey('search-name')), findsOneWidget);
  });
}
