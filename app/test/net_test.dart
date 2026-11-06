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
  testWidgets('Net menu opens status, not a placeholder', (t) async {
    final (_, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Net'));
    await t.pumpAndSettle();
    await t.tap(find.text('Status…'));
    await t.pumpAndSettle();
    expect(c.read(sectionProvider), Section.net);
    expect(find.textContaining('Coming to the app'), findsNothing);
    expect(find.textContaining('Not in the app yet'), findsNothing);
    expect(find.byKey(const ValueKey('net-running')), findsOneWidget);
    expect(find.byKey(const ValueKey('net-start')), findsOneWidget);
    expect(find.text('(made when the node first starts)'), findsOneWidget);
    await t.tap(find.byKey(const ValueKey('net-pane-peers')));
    await t.pumpAndSettle();
    expect(find.text('lab'), findsOneWidget);
    expect(find.textContaining('estpeer'), findsOneWidget);
  });

  testWidgets('shows a running node and its reachable addresses', (t) async {
    final (api, c) = await pump(t);
    api.net = const NetOverview(
      status: NetStatus(
        label: 'desk',
        peerId: '12D3KooWabc',
        fingerprint: '3f2a 9c1d 77e0 41b8 aa02',
        running: true,
        pid: 4242,
        startedAt: 1704067200000,
        nat: 'public',
        mdns: true,
        relayServer: false,
        addresses: ['/ip4/1.2.3.4/tcp/4741/p2p/12D3KooWabc'],
        connected: 2,
        exchangesActive: 1,
        incoming: 1,
        shared: 3,
      ),
      peers: [
        NetPeerInfo(
          peerId: '12D3KooWxyz',
          shortId: 'KooWxyz',
          label: 'ben',
          link: 'connected',
          source: 'mdns',
          caps: 'muninn',
        ),
      ],
    );
    c.read(sectionProvider.notifier).state = Section.net;
    await t.pumpAndSettle();
    await unlock(t);
    expect(find.text('desk'), findsOneWidget);
    expect(find.text('12D3KooWabc'), findsOneWidget);
    expect(find.textContaining('yes (pid 4242'), findsOneWidget);
    expect(find.text('3f2a 9c1d 77e0 41b8 aa02'), findsOneWidget);
    expect(find.text('public'), findsOneWidget);
    expect(find.text('/ip4/1.2.3.4/tcp/4741/p2p/12D3KooWabc'), findsOneWidget);
    expect(
      find.textContaining('1 active, 1 incoming offer(s)'),
      findsOneWidget,
    );
    expect(find.text('3 item(s)'), findsOneWidget);
    await t.tap(find.byKey(const ValueKey('net-pane-peers')));
    await t.pumpAndSettle();
    expect(find.text('ben'), findsOneWidget);
    expect(find.textContaining('connected'), findsOneWidget);
  });

  testWidgets('Start node marks the node running', (t) async {
    final (api, c) = await pump(t);
    await unlock(t);
    c.read(sectionProvider.notifier).state = Section.net;
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('net-start')));
    await t.pumpAndSettle();
    expect(api.netCalls, contains('start'));
    expect(find.byKey(const ValueKey('net-stop')), findsOneWidget);
    expect(find.textContaining('yes (pid 7'), findsOneWidget);
    expect(find.text('12D3KooWlocal'), findsOneWidget);
  });

  testWidgets('Invite or connect with an empty field shows a public link', (
    t,
  ) async {
    final (api, _) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Net'));
    await t.pumpAndSettle();
    await t.tap(find.text('Invite or connect…'));
    await t.pumpAndSettle();
    expect(find.byKey(const ValueKey('net-invite-or-connect')), findsOneWidget);
    await t.tap(find.byKey(const ValueKey('net-dialog-confirm')));
    await t.pumpAndSettle();
    expect(api.netCalls, contains('invite:'));
    expect(find.byKey(const ValueKey('net-invite-link')), findsOneWidget);
    expect(find.text('muninn://join/testhook'), findsWidgets);
    expect(find.textContaining('holds no secret'), findsOneWidget);
  });

  testWidgets('Share a project lists it on Shared', (t) async {
    final (api, c) = await pump(t);
    await unlock(t);
    c.read(sectionProvider.notifier).state = Section.net;
    c.read(netPaneProvider.notifier).state = NetPane.shared;
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('net-share')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('net-share-item')), 'Alpha');
    await t.tap(find.byKey(const ValueKey('net-share-confirm')));
    await t.pumpAndSettle();
    expect(api.netCalls, contains('share:project:Alpha'));
    expect(find.text('project Alpha'), findsOneWidget);
    expect(find.textContaining('everyone with an exchange'), findsOneWidget);
  });

  testWidgets('Add relay lists the address', (t) async {
    final (api, c) = await pump(t);
    await unlock(t);
    c.read(sectionProvider.notifier).state = Section.net;
    c.read(netPaneProvider.notifier).state = NetPane.relays;
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('net-relay-add')));
    await t.pumpAndSettle();
    await t.enterText(
      find.byKey(const ValueKey('net-relay-addr')),
      '/ip4/1.2.3.4/tcp/4741/p2p/12D3KooWC21S2Wxq3UCGcZYTsipGKjRitEar4nFARuvzpiUv2HH2',
    );
    await t.tap(find.byKey(const ValueKey('net-dialog-confirm')));
    await t.pumpAndSettle();
    expect(api.netCalls.any((c) => c.startsWith('relay-add:')), isTrue);
    expect(find.textContaining('/ip4/1.2.3.4/tcp/4741/p2p/'), findsWidgets);
  });
}
