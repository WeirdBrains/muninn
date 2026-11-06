import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:muninn/src/api.dart';
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
  testWidgets('New project creates one and lists it', (t) async {
    final (api, c) = await pump(t);
    await openProjects(t, c);
    expect(find.text('No projects: `mnnn project create NAME`'), findsNothing);
    await t.tap(find.byKey(const ValueKey('projects-new')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('project-name')), 'Desk');
    await t.enterText(
      find.byKey(const ValueKey('project-description')),
      'notes',
    );
    await t.tap(find.byKey(const ValueKey('project-save')));
    await t.pumpAndSettle();
    expect(api.createdProjects, ['Desk']);
    expect(find.text('Desk'), findsOneWidget);
    expect(find.text('notes'), findsOneWidget);
  });

  testWidgets('Projects menu opens New project', (t) async {
    final (api, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Projects'));
    await t.pumpAndSettle();
    await t.tap(find.text('New project…'));
    await t.pumpAndSettle();
    expect(c.read(sectionProvider), Section.projects);
    expect(find.byKey(const ValueKey('project-name')), findsOneWidget);
    await t.tap(find.byKey(const ValueKey('project-save')));
    await t.pumpAndSettle();
    expect(find.text('Name is required.'), findsOneWidget);
    expect(api.createdProjects, isEmpty);
  });

  testWidgets('rename and describe a project', (t) async {
    final (api, c) = await pump(t);
    await openProjects(t, c);
    await t.tap(find.byKey(const ValueKey('project-menu-p1')));
    await t.pumpAndSettle();
    await t.tap(find.text('Rename…'));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('project-name')), 'Beta');
    await t.tap(find.byKey(const ValueKey('project-save')));
    await t.pumpAndSettle();
    expect(api.updatedProjects, [(id: 'p1', name: 'Beta', description: null)]);
    expect(find.text('Beta'), findsOneWidget);

    await t.tap(find.byKey(const ValueKey('project-menu-p1')));
    await t.pumpAndSettle();
    await t.tap(find.text('Edit description…'));
    await t.pumpAndSettle();
    await t.enterText(
      find.byKey(const ValueKey('project-description')),
      'updated',
    );
    await t.tap(find.byKey(const ValueKey('project-save')));
    await t.pumpAndSettle();
    expect(api.updatedProjects.last, (
      id: 'p1',
      name: null,
      description: 'updated',
    ));
    expect(find.text('updated'), findsOneWidget);
  });

  testWidgets('delete a project after confirm', (t) async {
    final (api, c) = await pump(t);
    await openProjects(t, c);
    await t.tap(find.byKey(const ValueKey('project-menu-p1')));
    await t.pumpAndSettle();
    await t.tap(find.text('Delete…'));
    await t.pumpAndSettle();
    expect(
      find.textContaining('Journals stay as standalone journals'),
      findsOneWidget,
    );
    await t.tap(find.byKey(const ValueKey('project-delete-confirm')));
    await t.pumpAndSettle();
    expect(api.removedProjects, ['p1']);
    expect(find.text('Alpha'), findsNothing);
    expect(find.text('No projects yet'), findsOneWidget);
  });

  testWidgets('attach the current dataset and detach a series', (t) async {
    final (api, c) = await pump(t);
    await openProjects(t, c);
    await t.tap(find.text('Alpha'));
    await t.pumpAndSettle();
    expect(find.text('coinbase · BTC/USD'), findsOneWidget);

    await t.tap(find.byKey(const ValueKey('project-add-series')));
    await t
        .pump(); // SnackBar is showing; pumpAndSettle would wait for it to hide
    expect(find.text('Pick a dataset in Data first'), findsOneWidget);
    expect(api.addedSeries, isEmpty);
    await t.pumpAndSettle();

    c.read(selectedDatasetProvider.notifier).state = 'binance:ETH/USDT:3600';
    c.read(timeRangeProvider.notifier).state = const TimeRange(
      from: 1609459200000,
    );
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('project-add-series')));
    await t.pumpAndSettle();
    expect(api.addedSeries, hasLength(1));
    expect(api.addedSeries.single.projectId, 'p1');
    expect(api.addedSeries.single.d.symbol, 'ETH/USDT');
    expect(api.addedSeries.single.range, const TimeRange(from: 1609459200000));
    expect(find.text('binance · ETH/USDT'), findsOneWidget);

    await t.tap(find.byKey(const ValueKey('project-remove-series-1')));
    await t.pumpAndSettle();
    expect(api.removedSeries, [1]);
    expect(find.text('coinbase · BTC/USD'), findsNothing);
    expect(find.text('binance · ETH/USDT'), findsOneWidget);
  });
}
