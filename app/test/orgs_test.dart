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

void main() {
  testWidgets('Orgs menu opens the list, not a placeholder', (t) async {
    final (_, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Orgs'));
    await t.pumpAndSettle();
    await t.tap(find.text('Organizations…'));
    await t.pumpAndSettle();
    expect(c.read(sectionProvider), Section.orgs);
    expect(find.textContaining('Coming to the app'), findsNothing);
    expect(find.textContaining('Not in the app yet'), findsNothing);
    expect(find.text('Lab'), findsOneWidget);
    expect(
      find.textContaining('lab · private · members · 1 members'),
      findsOneWidget,
    );
    expect(find.text('Desk'), findsOneWidget);
    expect(find.textContaining('member as user at …abcdefgh'), findsOneWidget);
    expect(find.text('copy'), findsOneWidget);
  });

  testWidgets('expanding an org shows show fields', (t) async {
    final (_, c) = await pump(t);
    c.read(sectionProvider.notifier).state = Section.orgs;
    await t.pumpAndSettle();
    await unlock(t);
    await t.tap(find.byKey(const ValueKey('org-lab')));
    await t.pumpAndSettle();
    expect(find.text('we look'), findsOneWidget);
    expect(find.textContaining('alice  owner  member'), findsOneWidget);
    expect(find.textContaining('owner  rank 100'), findsOneWidget);
    expect(find.textContaining('member sign-in off'), findsOneWidget);
  });

  testWidgets('empty list points at the CLI', (t) async {
    final (api, c) = await pump(t);
    api.orgs = const OrgOverview();
    c.read(sectionProvider.notifier).state = Section.orgs;
    await t.pumpAndSettle();
    await unlock(t);
    expect(
      find.textContaining('New organization creates one here'),
      findsOneWidget,
    );
    expect(find.text('Lab'), findsNothing);
  });

  testWidgets('New organization dialog creates an org', (t) async {
    final (api, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Orgs'));
    await t.pumpAndSettle();
    await t.tap(find.text('New organization…'));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('org-create-name')), 'Nebula');
    await t.tap(find.text('Create'));
    await t.pumpAndSettle();
    expect(api.orgCalls, contains('create:Nebula'));
    expect(find.text('Nebula'), findsWidgets);
    expect(c.read(sectionProvider), Section.orgs);
  });

  testWidgets('Start and stop site', (t) async {
    final (api, c) = await pump(t);
    c.read(sectionProvider.notifier).state = Section.orgs;
    await t.pumpAndSettle();
    await unlock(t);
    expect(find.textContaining('Site not running'), findsOneWidget);
    await t.tap(find.byKey(const ValueKey('org-host-start')));
    await t.pumpAndSettle();
    expect(api.orgCalls, contains('host-start'));
    expect(
      find.textContaining('Site at https://127.0.0.1:8443/'),
      findsOneWidget,
    );
    await t.tap(find.byKey(const ValueKey('org-host-stop')));
    await t.pumpAndSettle();
    expect(api.orgCalls, contains('host-stop'));
    expect(find.textContaining('Site not running'), findsOneWidget);
  });

  testWidgets('expanding an org offers edit, invite and remove', (t) async {
    final (api, c) = await pump(t);
    c.read(sectionProvider.notifier).state = Section.orgs;
    await t.pumpAndSettle();
    await unlock(t);
    await t.tap(find.byKey(const ValueKey('org-lab')));
    await t.pumpAndSettle();
    expect(find.byKey(const ValueKey('org-edit-lab')), findsOneWidget);
    expect(find.byKey(const ValueKey('org-invite-lab')), findsOneWidget);
    await t.tap(find.byKey(const ValueKey('org-invite-lab')));
    await t.pumpAndSettle();
    await t.tap(find.text('OK'));
    await t.pumpAndSettle();
    expect(api.orgCalls.any((c) => c.startsWith('invite:lab')), isTrue);
    expect(find.byKey(const ValueKey('org-result-text')), findsOneWidget);
  });

  testWidgets('join from the menu', (t) async {
    final (api, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Orgs'));
    await t.pumpAndSettle();
    await t.tap(find.text('Join…'));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('org-join-token')), 'TOKEN');
    await t.tap(find.text('OK'));
    await t.pumpAndSettle();
    expect(api.orgCalls, contains('join:TOKEN'));
    expect(find.textContaining('joined Joined as user'), findsWidgets);
  });
}
