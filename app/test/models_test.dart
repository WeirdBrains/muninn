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
  testWidgets('model designer creates, edits fields and applies a draft', (
    t,
  ) async {
    final (api, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Projects'));
    await t.pumpAndSettle();
    await t.ensureVisible(find.text('Models…'));
    await t.tap(find.text('Models…'));
    await t.pumpAndSettle();
    expect(c.read(projectPaneProvider), ProjectPane.models);
    expect(find.textContaining('Coming to the app'), findsNothing);

    await t.tap(find.byKey(const ValueKey('model-new')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('model-name')), 'signals');
    await t.enterText(find.byKey(const ValueKey('model-fields')), 'memo:text');
    await t.tap(find.byKey(const ValueKey('model-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('model-create:signals'));
    expect(find.text('signals'), findsWidgets);
    expect(find.textContaining('memo'), findsWidgets);

    await t.tap(find.byKey(const ValueKey('model-add-field')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('field-spec')), 'note:text');
    await t.tap(find.byKey(const ValueKey('field-spec-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls.any((c) => c.startsWith('model-edit:')), isTrue);
    expect(find.textContaining('note'), findsWidgets);

    await t.tap(find.byKey(const ValueKey('model-apply')));
    await t.pumpAndSettle();
    expect(
      api.extraCalls,
      contains('model-apply:${api.modelList.first.id}:false'),
    );
    expect(api.modelList.first.hasDraft, isFalse);
    expect(api.modelList.first.version, 1);
  });

  testWidgets('model designer attaches a rule to the open model', (t) async {
    final (api, c) = await pump(t);
    await unlock(t);
    await api.modelCreate('signals', fields: 'memo:text', draft: false);
    c.read(projectPaneProvider.notifier).state = ProjectPane.models;
    c.read(viewingModelIdProvider.notifier).state = api.modelList.first.id;
    c.read(sectionProvider.notifier).state = Section.projects;
    await t.pumpAndSettle();

    await t.tap(find.byKey(const ValueKey('model-tab-rules')));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('model-rule-new')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('model-rule-name')), 'feed');
    await t.enterText(
      find.byKey(const ValueKey('model-rule-then')),
      'status hi',
    );
    await t.tap(find.byKey(const ValueKey('model-rule-save')));
    await t.pumpAndSettle();
    expect(api.ruleList, isNotEmpty);
    expect(api.ruleList.first.home, 'model signals');
  });

  testWidgets('model designer restores a dropped field with the same id', (
    t,
  ) async {
    final (api, c) = await pump(t);
    await unlock(t);
    await api.modelCreate('signals', fields: 'memo:text', draft: false);
    final model = api.modelList.first;
    final memo = model.designFields.firstWhere((f) => f.name == 'memo');
    await api.modelEdit(model.id, drop: [memo.name]);
    c.read(projectPaneProvider.notifier).state = ProjectPane.models;
    c.read(viewingModelIdProvider.notifier).state = model.id;
    c.read(sectionProvider.notifier).state = Section.projects;
    await t.pumpAndSettle();

    expect(api.modelList.first.designFields.first.dropped, isTrue);
    await t.tap(find.byKey(ValueKey('design-field-action-${memo.id}')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('model-restore:${model.id}:${memo.id}'));
    final restored = api.modelList.first.designFields.firstWhere(
      (f) => f.id == memo.id,
    );
    expect(restored.dropped, isFalse);
    expect(restored.name, 'memo');
  });

  testWidgets('model designer imports a template and edits records', (t) async {
    final (api, c) = await pump(t);
    await unlock(t);
    c.read(projectPaneProvider.notifier).state = ProjectPane.models;
    c.read(sectionProvider.notifier).state = Section.projects;
    await t.pumpAndSettle();

    await t.tap(find.byKey(const ValueKey('model-import')));
    await t.pumpAndSettle();
    await t.enterText(
      find.byKey(const ValueKey('model-import-source')),
      'builtin:bars',
    );
    await t.tap(find.byKey(const ValueKey('model-import-source-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('model-import:builtin:bars::false'));
    expect(api.modelList.any((m) => m.name == 'imported'), isTrue);

    final model = api.modelList.firstWhere((m) => m.name == 'imported');
    c.read(viewingModelIdProvider.notifier).state = model.id;
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('model-tab-records')));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('model-record-new')));
    await t.pumpAndSettle();
    await t.enterText(
      find.byKey(const ValueKey('record-values')),
      'memo=hello',
    );
    await t.tap(find.byKey(const ValueKey('record-values-save')));
    await t.pumpAndSettle();
    expect(
      api.extraCalls,
      contains('model-record-create:${model.id}:memo=hello'),
    );
    expect(api.recordList, isNotEmpty);
    expect(find.text('hello'), findsWidgets);
  });

  testWidgets('model designer dry-runs a rule on the last history action', (
    t,
  ) async {
    final (api, c) = await pump(t);
    await unlock(t);
    await api.modelCreate('signals', fields: 'memo:text', draft: false);
    await api.ruleCreate(
      'feed',
      on: 'import',
      then: 'status hi',
      home: 'model:signals',
    );
    c.read(projectPaneProvider.notifier).state = ProjectPane.models;
    c.read(viewingModelIdProvider.notifier).state = api.modelList.first.id;
    c.read(sectionProvider.notifier).state = Section.projects;
    await t.pumpAndSettle();

    await t.tap(find.byKey(const ValueKey('model-tab-rules')));
    await t.pumpAndSettle();
    final id = api.ruleList.first.id;
    await t.tap(find.byKey(ValueKey('model-rule-dry-$id')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('rule-dry-run:$id'));
    expect(find.textContaining('would show'), findsOneWidget);
    await t.tap(find.text('Close'));
    await t.pumpAndSettle();
    await t.tap(find.byKey(ValueKey('model-rule-try-$id')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('model-rule-try:$id'));
  });
}
