import 'package:flutter/material.dart';
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

Future<void> unlock(WidgetTester t) async {
  await t.enterText(find.byType(TextField), 'pw');
  await t.tap(find.text('Unlock'));
  await t.pumpAndSettle();
}

void main() {
  testWidgets('Code spaces menu creates a space, not a placeholder', (t) async {
    final (api, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Projects'));
    await t.pumpAndSettle();
    await t.tap(find.text('Code spaces…'));
    await t.pumpAndSettle();
    expect(c.read(projectPaneProvider), ProjectPane.code);
    expect(find.textContaining('Coming to the app'), findsNothing);
    await t.tap(find.byKey(const ValueKey('code-new')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('code-name')), 'mean');
    await t.tap(find.byKey(const ValueKey('code-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('code-create:mean:math'));
    expect(find.text('mean'), findsOneWidget);
    expect(find.textContaining('not approved'), findsOneWidget);
  });

  testWidgets('Pipes menu lists none then runs a pipe', (t) async {
    final (api, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Projects'));
    await t.pumpAndSettle();
    await t.tap(find.text('Pipes (live)…'));
    await t.pumpAndSettle();
    expect(c.read(projectPaneProvider), ProjectPane.pipes);
    expect(find.textContaining('Coming to the app'), findsNothing);
    expect(find.textContaining('No pipes yet'), findsOneWidget);
    await t.tap(find.byKey(const ValueKey('pipe-run')));
    await t.pumpAndSettle();
    await t.enterText(
      find.byKey(const ValueKey('pipe-source')),
      'source:coinbase',
    );
    await t.enterText(find.byKey(const ValueKey('pipe-steps')), 'code:to-bars');
    await t.tap(find.byKey(const ValueKey('pipe-run-confirm')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('pipe-run:source:coinbase:code:to-bars'));
    expect(find.textContaining('pipe done'), findsOneWidget);
  });

  testWidgets('Pipelines menu creates a graph, not a placeholder', (t) async {
    final (api, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Projects'));
    await t.pumpAndSettle();
    await t.tap(find.text('Pipelines…'));
    await t.pumpAndSettle();
    expect(c.read(projectPaneProvider), ProjectPane.pipelines);
    expect(find.textContaining('Coming to the app'), findsNothing);
    await t.tap(find.byKey(const ValueKey('pipeline-new')));
    await t.pumpAndSettle();
    await t.enterText(
      find.byKey(const ValueKey('pipeline-name')),
      'bars-from-ingest',
    );
    await t.enterText(
      find.byKey(const ValueKey('pipeline-nodes')),
      'src=source:coinbase\nbars=code:to-bars\nout=model:bars',
    );
    await t.enterText(
      find.byKey(const ValueKey('pipeline-edges')),
      'src>bars\nbars>out',
    );
    await t.tap(find.byKey(const ValueKey('pipeline-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('pipeline-create:bars-from-ingest'));
    expect(find.text('bars-from-ingest'), findsOneWidget);
  });
}
