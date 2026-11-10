import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:muninn/src/api.dart';
import 'package:muninn/src/app.dart';
import 'package:muninn/src/state.dart';
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

void main() {
  testWidgets('Code spaces menu creates a space, not a placeholder', (t) async {
    final (api, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Code'));
    await t.pumpAndSettle();
    await t.tap(find.text('Code…'));
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

    await t.tap(find.byType(PopupMenuButton<String>));
    await t.pumpAndSettle();
    await t.tap(find.text('Result tile…'));
    await t.pumpAndSettle();
    expect(c.read(sectionProvider), Section.data);
    expect(
      c.read(workspaceProvider).tiles.any((t) => t.frameKey == 'code:mean'),
      isTrue,
    );
  });

  testWidgets('Pipes menu lists none then runs a pipe', (t) async {
    final (api, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Code'));
    await t.pumpAndSettle();
    await t.tap(find.text('Live pipes…'));
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
    await t.tap(find.text('Code'));
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

  testWidgets('Code space can be given to an owned organization', (t) async {
    final (api, _) = await pump(t);
    api.codeList = const [CodeSpaceInfo(id: 'c1', shortId: 'c1', name: 'calc')];
    await unlock(t);
    await t.tap(find.text('Code'));
    await t.pumpAndSettle();
    await t.tap(find.text('Code…'));
    await t.pumpAndSettle();
    await t.tap(find.byType(PopupMenuButton<String>));
    await t.pumpAndSettle();
    await t.tap(find.text('Give to org…'));
    await t.pumpAndSettle();
    expect(find.textContaining('Lab'), findsWidgets);
    await t.tap(find.byKey(const ValueKey('code-give-save')));
    await t.pumpAndSettle();
    expect(api.orgCalls, contains('resource-add:lab:code:c1'));
  });

  testWidgets('Pipe stdin, --format and --chunk match the CLI flags', (
    t,
  ) async {
    final (api, _) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Code'));
    await t.pumpAndSettle();
    await t.tap(find.text('Live pipes…'));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('pipe-run')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('pipe-source')), '-');
    await t.pump();
    expect(find.byKey(const ValueKey('pipe-stdin')), findsOneWidget);
    await t.enterText(
      find.byKey(const ValueKey('pipe-stdin')),
      'a,b\n1,2\n3,4',
    );
    await t.ensureVisible(find.byKey(const ValueKey('pipe-format')));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('pipe-format')));
    await t.pumpAndSettle();
    await t.tap(find.text('csv').last);
    await t.pumpAndSettle();
    await t.ensureVisible(find.byKey(const ValueKey('pipe-chunk')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('pipe-chunk')), '2');
    await t.ensureVisible(find.byKey(const ValueKey('pipe-run-confirm')));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('pipe-run-confirm')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('pipe-run:-:'));
    expect(
      api.extraCalls.any(
        (c) =>
            c.startsWith('pipe-stdin:-:format=csv:chunk=2') &&
            c.contains('text=a,b'),
      ),
      isTrue,
      reason: '${api.extraCalls}',
    );
  });

  testWidgets('Pipeline tee, end-in-model and monitor match the TUI canvas', (
    t,
  ) async {
    final (api, c) = await pump(t);
    api.pipelineList = const [
      PipelineInfo(
        id: 'p1',
        shortId: 'p1',
        name: 'feed',
        nodeSpecs: ['src=source:coinbase', 'bars=code:to-bars'],
        edgeSpecs: ['src>bars'],
        graphLines: ['src = source:coinbase → bars', 'bars = code:to-bars'],
        nodes: 2,
      ),
    ];
    await unlock(t);
    await t.tap(find.text('Code'));
    await t.pumpAndSettle();
    await t.tap(find.text('Pipelines…'));
    await t.pumpAndSettle();
    expect(find.text('feed'), findsOneWidget);
    await t.tap(find.byType(PopupMenuButton<String>));
    await t.pumpAndSettle();
    await t.tap(find.text('Tee…'));
    await t.pumpAndSettle();
    await t.enterText(
      find.byKey(const ValueKey('pipeline-tee-sink')),
      'journal:Raw',
    );
    await t.tap(find.byKey(const ValueKey('pipeline-tee-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('pipeline-tee:p1:src:journal:Raw'));

    await t.tap(find.byType(PopupMenuButton<String>));
    await t.pumpAndSettle();
    await t.tap(find.text('End in model…'));
    await t.pumpAndSettle();
    await t.enterText(
      find.byKey(const ValueKey('pipeline-end-model')),
      'ticks',
    );
    await t.tap(find.byKey(const ValueKey('pipeline-end-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('pipeline-end-in-model:p1:bars:ticks'));

    await t.tap(find.byType(PopupMenuButton<String>));
    await t.pumpAndSettle();
    await t.tap(find.text('Monitor…'));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('pipeline-monitor:p1'));
    expect(c.read(sectionProvider), Section.data);
    expect(
      c.read(workspaceProvider).tiles.any((t) => t.frameKey == 'monitor:p1'),
      isTrue,
    );
    expect(find.textContaining('Monitor · feed'), findsOneWidget);
    expect(find.byKey(const ValueKey('pipeline-monitor-src')), findsOneWidget);
  });
}
