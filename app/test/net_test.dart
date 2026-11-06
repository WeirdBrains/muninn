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
    expect(find.textContaining('no (start with'), findsOneWidget);
    expect(find.text('(made when the node first starts)'), findsOneWidget);
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
    expect(find.text('ben'), findsOneWidget);
    expect(find.textContaining('connected'), findsOneWidget);
  });
}
