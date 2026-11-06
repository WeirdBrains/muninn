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

Future<void> press(WidgetTester t, Finder f) async {
  final w = t.widget(f);
  if (w is! ButtonStyleButton) {
    throw StateError('expected a button, got ${w.runtimeType}');
  }
  w.onPressed!.call();
  await t.pumpAndSettle();
}

Future<void> unlock(WidgetTester t) async {
  await t.enterText(find.byType(TextField), 'pw');
  await t.tap(find.text('Unlock'));
  await t.pumpAndSettle();
}

Future<void> openLab(WidgetTester t, ProviderContainer c) async {
  c.read(sectionProvider.notifier).state = Section.orgs;
  await t.pumpAndSettle();
  await unlock(t);
  await t.tap(find.byKey(const ValueKey('org-lab')));
  await t.pumpAndSettle();
}

void main() {
  testWidgets('expanding an org offers templates, domains and endpoints', (
    t,
  ) async {
    final (_, c) = await pump(t);
    await openLab(t, c);
    expect(find.text('Templates'), findsOneWidget);
    expect(find.text('Domains'), findsOneWidget);
    expect(find.text('Certificate'), findsOneWidget);
    expect(find.text('Endpoints'), findsOneWidget);
    expect(find.textContaining('Coming to the app'), findsNothing);
    expect(find.byKey(const ValueKey('org-tpl-new-lab')), findsOneWidget);
    expect(find.byKey(const ValueKey('org-domain-add-lab')), findsOneWidget);
    expect(find.byKey(const ValueKey('org-ep-add-lab')), findsOneWidget);
    expect(find.byKey(const ValueKey('org-cert-set-lab')), findsOneWidget);
  });

  testWidgets('New template creates one, not a placeholder', (t) async {
    final (api, c) = await pump(t);
    await openLab(t, c);
    await press(t, find.byKey(const ValueKey('org-tpl-new-lab')));
    await t.enterText(find.byKey(const ValueKey('org-tpl-name')), 'home');
    await t.tap(find.byKey(const ValueKey('org-tpl-save')));
    await t.pumpAndSettle();
    expect(api.orgCalls, contains('template-save:lab:home'));
    expect(find.textContaining('home (in use)'), findsOneWidget);
  });

  testWidgets('Add domain records it', (t) async {
    final (api, c) = await pump(t);
    await openLab(t, c);
    await press(t, find.byKey(const ValueKey('org-domain-add-lab')));
    await t.enterText(
      find.byKey(const ValueKey('org-domain')),
      'lab.example.org',
    );
    await t.tap(find.byKey(const ValueKey('org-domain-save')));
    await t.pumpAndSettle();
    expect(api.orgCalls, contains('domain-add:lab:lab.example.org'));
    expect(find.textContaining('lab.example.org'), findsWidgets);
  });

  testWidgets('Add endpoint records it', (t) async {
    final (api, c) = await pump(t);
    await openLab(t, c);
    await press(t, find.byKey(const ValueKey('org-ep-add-lab')));
    await t.enterText(find.byKey(const ValueKey('org-ep-name')), 'query');
    await t.enterText(find.byKey(const ValueKey('org-ep-target')), 'signals');
    await t.tap(find.byKey(const ValueKey('org-ep-save')));
    await t.pumpAndSettle();
    expect(api.orgCalls, contains('endpoint-add:lab:query:records:signals'));
    expect(find.textContaining('query  records'), findsOneWidget);
  });

  testWidgets('Browse menu lists a peer org, not a placeholder', (t) async {
    final (api, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Orgs'));
    await t.pumpAndSettle();
    await t.ensureVisible(find.text('Browse…'));
    await t.tap(find.text('Browse…'));
    await t.pumpAndSettle();
    expect(c.read(sectionProvider), Section.orgs);
    await t.enterText(find.byKey(const ValueKey('org-browse-target')), 'peer');
    await t.tap(find.byKey(const ValueKey('org-browse-go')));
    await t.pumpAndSettle();
    expect(api.orgCalls, contains('browse:peer:'));
    expect(find.textContaining('Peer Lab'), findsOneWidget);
    expect(find.textContaining('Coming to the app'), findsNothing);
  });

  testWidgets('Certificates menu shows status without PEM', (t) async {
    final (api, _) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Orgs'));
    await t.pumpAndSettle();
    await t.ensureVisible(find.text('Certificates…'));
    await t.tap(find.text('Certificates…'));
    await t.pumpAndSettle();
    expect(find.textContaining('self-signed'), findsWidgets);
    expect(find.textContaining('BEGIN CERTIFICATE'), findsNothing);
    expect(find.textContaining('BEGIN PRIVATE'), findsNothing);
    expect(find.textContaining('Coming to the app'), findsNothing);
    expect(api, isNotNull);
  });

  testWidgets('joined org offers fetch, give, mirror and sign-in', (t) async {
    final (api, c) = await pump(t);
    c.read(sectionProvider.notifier).state = Section.orgs;
    await t.pumpAndSettle();
    await unlock(t);
    expect(find.byKey(const ValueKey('org-fetch-desk')), findsOneWidget);
    expect(find.byKey(const ValueKey('org-give-desk')), findsOneWidget);
    expect(find.byKey(const ValueKey('org-mirror-desk')), findsOneWidget);
    expect(find.byKey(const ValueKey('org-signin-desk')), findsOneWidget);
    await t.tap(find.byKey(const ValueKey('org-signin-desk')));
    await t.pumpAndSettle();
    await t.enterText(
      find.byKey(const ValueKey('org-signin-site')),
      'https://desk.example.org',
    );
    await t.tap(find.byKey(const ValueKey('org-signin-go')));
    await t.pumpAndSettle();
    expect(api.orgCalls, contains('sign-in:desk:https://desk.example.org'));
    expect(find.textContaining('sign-in?auth='), findsOneWidget);
  });
}
