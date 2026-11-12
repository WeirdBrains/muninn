import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:muninn/src/app.dart';
import 'package:muninn/src/state.dart';
import 'package:muninn/src/workspace.dart';
import 'package:muninn/src/views/data.dart';
import 'package:muninn/src/views/projects.dart';

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

void main() {
  testWidgets('tapping a Home project tile opens it in Projects', (t) async {
    final (_, c) = await pump(t, const Size(1280, 800));
    expect(c.read(sectionProvider), Section.home);
    await unlock(t);

    await t.tap(find.byKey(const ValueKey('home-project-p1')));
    await t.pumpAndSettle();
    expect(c.read(sectionProvider), Section.projects);
    expect(c.read(selectedProjectProvider)?.id, 'p1');
    expect(c.read(selectedProjectProvider)?.name, 'Alpha');
    expect(find.byType(ProjectsView), findsOneWidget);
    expect(find.text('Alpha'), findsWidgets);
  });

  testWidgets('tapping a Home dataset tile opens it in the Data workspace', (
    t,
  ) async {
    final (_, c) = await pump(t, const Size(1280, 800));
    await unlock(t);

    await t.tap(
      find.byKey(const ValueKey('home-dataset-coinbase:BTC/USD:300')),
    );
    await t.pumpAndSettle();
    expect(c.read(sectionProvider), Section.data);
    expect(c.read(selectedDatasetProvider), 'coinbase:BTC/USD:300');
    expect(
      c.read(workspaceProvider).focused.datasetKey,
      'coinbase:BTC/USD:300',
    );
    expect(c.read(workspaceProvider).tiles, hasLength(1));
    expect(find.byType(DataView), findsOneWidget);
    expect(find.byKey(const ValueKey('data-tile-1')), findsOneWidget);
  });
}
