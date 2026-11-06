import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:muninn/src/app.dart';
import 'package:muninn/src/state.dart';
import 'package:muninn/src/views/journal_extras.dart';

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

Future<void> press(WidgetTester t, Finder f) async {
  final w = t.widget(f);
  if (w is! ButtonStyleButton) {
    throw StateError('expected a button, got ${w.runtimeType}');
  }
  w.onPressed!.call();
  await t.pumpAndSettle();
}

Future<(FakeApi, ProviderContainer)> openNotes(WidgetTester t) async {
  final (api, c) = await pump(t);
  c.read(sectionProvider.notifier).state = Section.projects;
  await t.pumpAndSettle();
  await unlock(t);
  await t.tap(find.byKey(const ValueKey('projects-pane-journals')));
  await t.pumpAndSettle();
  await t.tap(find.byKey(const ValueKey('journals-new')));
  await t.pumpAndSettle();
  await t.enterText(find.byKey(const ValueKey('journal-name')), 'Desk notes');
  await t.tap(find.byKey(const ValueKey('journal-save')));
  await t.pumpAndSettle();
  await t.tap(find.byKey(const ValueKey('journal-add-entry')));
  await t.pumpAndSettle();
  await t.enterText(find.byKey(const ValueKey('entry-title')), 'Monday');
  await t.enterText(
    find.byKey(const ValueKey('entry-body')),
    'the quick brown fox',
  );
  await t.tap(find.byKey(const ValueKey('entry-save')));
  await t.pumpAndSettle();
  return (api, c);
}

void main() {
  testWidgets('journal offers highlights, images, copy and export', (t) async {
    await openNotes(t);
    expect(find.byKey(const ValueKey('journals-import')), findsOneWidget);
    expect(find.byKey(const ValueKey('journal-export')), findsOneWidget);
    expect(find.byKey(const ValueKey('journal-print')), findsOneWidget);
    expect(find.byKey(const ValueKey('entry-highlight-e1')), findsOneWidget);
    expect(find.byKey(const ValueKey('entry-attach-e1')), findsOneWidget);
    expect(find.byKey(const ValueKey('entry-copy-e1')), findsOneWidget);
    expect(find.byKey(const ValueKey('entry-note-words-e1')), findsOneWidget);
    expect(find.byKey(const ValueKey('entry-word-e1-0')), findsOneWidget);
    expect(find.byKey(const ValueKey('entry-word-e1-1')), findsOneWidget);
    expect(find.byKey(const ValueKey('entry-word-e1-3')), findsOneWidget);
    expect(find.textContaining('Coming to the app'), findsNothing);
  });

  test('journalWords splits on whitespace like the TUI', () {
    expect(journalWords('the quick brown fox'), [
      (0, 3),
      (4, 9),
      (10, 15),
      (16, 19),
    ]);
    expect(journalSlice('the quick brown fox', 4, 15), 'quick brown');
  });

  testWidgets('tapping a word prefills the Highlight dialog', (t) async {
    final (api, _) = await openNotes(t);
    await t.tap(find.byKey(const ValueKey('entry-word-e1-1')));
    await t.pumpAndSettle();
    await press(t, find.byKey(const ValueKey('entry-highlight-e1')));
    expect(
      t.widget<TextField>(find.byKey(const ValueKey('journal-hl-words'))).controller?.text,
      'quick',
    );
    await t.tap(find.byKey(const ValueKey('journal-hl-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('highlight:e1:quick'));
  });

  testWidgets('Highlight records selected words, not a placeholder', (t) async {
    final (api, _) = await openNotes(t);
    await press(t, find.byKey(const ValueKey('entry-highlight-e1')));
    await t.enterText(
      find.byKey(const ValueKey('journal-hl-words')),
      'quick brown',
    );
    await t.tap(find.byKey(const ValueKey('journal-hl-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('highlight:e1:quick brown'));
    expect(find.textContaining('highlight on "quick brown"'), findsOneWidget);
    expect(find.textContaining('Coming to the app'), findsNothing);
  });

  testWidgets('Copy entries into a new journal', (t) async {
    final (api, _) = await openNotes(t);
    await press(t, find.byKey(const ValueKey('entry-copy-e1')));
    await t.enterText(find.byKey(const ValueKey('journal-copy-new')), 'Copied');
    await t.tap(find.byKey(const ValueKey('journal-copy-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('copy:e1::Copied'));
    expect(api.extraCalls, contains('journal-create:Copied'));
  });

  testWidgets('Export journal writes a document path', (t) async {
    final (api, _) = await openNotes(t);
    await press(t, find.byKey(const ValueKey('journal-export')));
    await t.enterText(
      find.byKey(const ValueKey('journal-export-out')),
      '/tmp/desk-notes.json',
    );
    await t.tap(find.byKey(const ValueKey('journal-export-save')));
    await t.pumpAndSettle();
    expect(
      api.extraCalls,
      contains('journal-export:j1:pdf:/tmp/desk-notes.json'),
    );
  });

  testWidgets('Attach image from a path, without sending bytes', (t) async {
    final (api, _) = await openNotes(t);
    await press(t, find.byKey(const ValueKey('entry-attach-e1')));
    await t.enterText(
      find.byKey(const ValueKey('journal-attach-path')),
      '/tmp/dot.png',
    );
    await t.tap(find.byKey(const ValueKey('journal-attach-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('attach:e1:/tmp/dot.png'));
    expect(find.text('dot.png'), findsOneWidget);
    expect(find.byKey(const ValueKey('image-thumb-img1')), findsOneWidget);
    expect(api.extraCalls, contains('image-thumb:img1'));
  });

  testWidgets('Import journal from the pane', (t) async {
    final (api, c) = await pump(t);
    c.read(sectionProvider.notifier).state = Section.projects;
    await t.pumpAndSettle();
    await unlock(t);
    await t.tap(find.byKey(const ValueKey('projects-pane-journals')));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('journals-import')));
    await t.pumpAndSettle();
    await t.enterText(
      find.byKey(const ValueKey('journal-import-path')),
      '/tmp/notes.json',
    );
    await t.enterText(
      find.byKey(const ValueKey('journal-import-name')),
      'Brought in',
    );
    await t.tap(find.byKey(const ValueKey('journal-import-save')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('journal-import:/tmp/notes.json'));
    expect(api.extraCalls, contains('journal-create:Brought in'));
    expect(find.text('Brought in'), findsOneWidget);
  });
}
