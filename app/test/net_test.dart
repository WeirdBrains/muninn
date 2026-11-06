import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:muninn/src/api.dart';
import 'package:muninn/src/app.dart';
import 'package:muninn/src/state.dart';

import 'fake_api.dart';

Future<(FakeApi, ProviderContainer)> pump(WidgetTester t) async {
  t.view.physicalSize = const Size(1280, 1024);
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

  testWidgets('Settings menu opens the form, not a placeholder', (t) async {
    final (api, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Net'));
    await t.pumpAndSettle();
    await t.tap(find.text('Settings…'));
    await t.pumpAndSettle();
    expect(c.read(netPaneProvider), NetPane.settings);
    expect(find.textContaining('Coming to the app'), findsNothing);
    expect(find.textContaining('Not in the app yet'), findsNothing);
    expect(find.byKey(const ValueKey('net-settings-save')), findsOneWidget);
    await t.enterText(find.byKey(const ValueKey('net-config-label')), 'desk');
    await t.ensureVisible(find.byKey(const ValueKey('net-settings-save')));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('net-settings-save')));
    await t.pumpAndSettle();
    expect(api.netCalls, contains('config-set:desk'));
    expect(find.textContaining('saved network settings'), findsOneWidget);
  });

  testWidgets('Connections menu trusts a user', (t) async {
    final (api, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Net'));
    await t.pumpAndSettle();
    await t.tap(find.text('Connections…'));
    await t.pumpAndSettle();
    expect(c.read(netPaneProvider), NetPane.connections);
    expect(find.textContaining('Coming to the app'), findsNothing);
    expect(find.text('bob'), findsOneWidget);
    await t.tap(find.byKey(const ValueKey('net-trust-bobuser')));
    await t.pumpAndSettle();
    expect(api.netCalls, contains('trust:u2:true'));
    expect(find.text('Untrust'), findsOneWidget);
  });

  testWidgets('Nodes menu lists no exchanges yet', (t) async {
    final (_, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Net'));
    await t.pumpAndSettle();
    await t.tap(find.text('Nodes…'));
    await t.pumpAndSettle();
    expect(c.read(netPaneProvider), NetPane.exchanges);
    expect(find.textContaining('Coming to the app'), findsNothing);
    expect(find.text('No key exchanges yet.'), findsOneWidget);
  });

  testWidgets('Dark shards menu shows empty entrusted and held lists', (
    t,
  ) async {
    final (_, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Net'));
    await t.pumpAndSettle();
    await t.tap(find.text('Dark shards…'));
    await t.pumpAndSettle();
    expect(c.read(netPaneProvider), NetPane.shards);
    expect(find.textContaining('Coming to the app'), findsNothing);
    expect(find.text('No dark shards entrusted.'), findsOneWidget);
    expect(find.text('No dark shards held.'), findsOneWidget);
    expect(find.byKey(const ValueKey('net-shard-entrust')), findsOneWidget);
  });

  testWidgets('Network search runs a saved live search', (t) async {
    final (api, c) = await pump(t);
    api.searchList = [
      const SearchInfo(
        id: 's1',
        shortId: 's1short',
        name: 'wide',
        query: 'BTC',
      ),
    ];
    await unlock(t);
    await t.tap(find.text('Net'));
    await t.pumpAndSettle();
    await t.tap(find.text('Network search…'));
    await t.pumpAndSettle();
    expect(c.read(netPaneProvider), NetPane.search);
    expect(find.textContaining('Coming to the app'), findsNothing);
    await t.tap(find.byKey(const ValueKey('net-search-run')));
    await t.pumpAndSettle();
    expect(api.netCalls, contains('search:wide:all'));
    expect(find.text('BTC/USD 5m match'), findsOneWidget);
  });

  testWidgets('Broadcast dialog sends a topic and text', (t) async {
    final (api, _) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Net'));
    await t.pumpAndSettle();
    await t.tap(find.text('Broadcast…'));
    await t.pumpAndSettle();
    expect(find.textContaining('Coming to the app'), findsNothing);
    await t.enterText(
      find.byKey(const ValueKey('net-broadcast-topic')),
      'hello',
    );
    await t.enterText(
      find.byKey(const ValueKey('net-broadcast-text')),
      'world',
    );
    await t.tap(find.byKey(const ValueKey('net-broadcast-confirm')));
    await t.pumpAndSettle();
    expect(api.netCalls, contains('broadcast:nodes:hello:world'));
    expect(find.textContaining('sent to 0 node(s)'), findsOneWidget);
  });

  testWidgets('Broadcast can target org:SLUG', (t) async {
    final (api, _) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Net'));
    await t.pumpAndSettle();
    await t.tap(find.text('Broadcast…'));
    await t.pumpAndSettle();
    await t.enterText(
      find.byKey(const ValueKey('net-broadcast-topic')),
      'hello',
    );
    await t.enterText(
      find.byKey(const ValueKey('net-broadcast-text')),
      'world',
    );
    await t.enterText(find.byKey(const ValueKey('net-broadcast-org')), 'lab');
    await t.tap(find.byKey(const ValueKey('net-broadcast-confirm')));
    await t.pumpAndSettle();
    expect(api.netCalls, contains('broadcast:org:lab:hello:world'));
  });

  testWidgets('Network search --only a subset of peers', (t) async {
    final (api, _) = await pump(t);
    api.searchList = [
      const SearchInfo(
        id: 's1',
        shortId: 's1short',
        name: 'wide',
        query: 'BTC',
      ),
    ];
    api.exchanges = const [
      NetExchangeInfo(id: 'ex1', shortId: 'ex1s', title: 'ben', active: true),
    ];
    await unlock(t);
    await t.tap(find.text('Net'));
    await t.pumpAndSettle();
    await t.tap(find.text('Network search…'));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('net-search-only-ex1')));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('net-search-run')));
    await t.pumpAndSettle();
    expect(api.netCalls, contains('search:wide:all:only=ex1'));
  });

  testWidgets('Export exchanges writes a sealed backup', (t) async {
    final (api, _) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Net'));
    await t.pumpAndSettle();
    await t.tap(find.text('Nodes…'));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('net-exchange-export')));
    await t.pumpAndSettle();
    await t.enterText(
      find.byKey(const ValueKey('net-exchange-export-out')),
      '/tmp/muninn-exchanges.age',
    );
    await t.enterText(
      find.byKey(const ValueKey('net-exchange-export-password')),
      'pw',
    );
    await t.tap(find.byKey(const ValueKey('net-exchange-export-confirm')));
    await t.pumpAndSettle();
    expect(api.netCalls, contains('exchange-export:/tmp/muninn-exchanges.age'));
    expect(
      find.textContaining('wrote /tmp/muninn-exchanges.age'),
      findsOneWidget,
    );
  });

  testWidgets('Restore exchanges asks before replacing the node identity', (
    t,
  ) async {
    final (api, _) = await pump(t);
    api.exchangePeek = const NetExchangeBackupPeek(
      peerId: '12D3KooWPeer',
      exchanges: 1,
      identityReplaced: true,
    );
    await unlock(t);
    await t.tap(find.text('Net'));
    await t.pumpAndSettle();
    await t.tap(find.text('Restore exchanges…'));
    await t.pumpAndSettle();
    await t.enterText(
      find.byKey(const ValueKey('net-exchange-restore-file')),
      '/tmp/muninn-exchanges.age',
    );
    await t.enterText(
      find.byKey(const ValueKey('net-exchange-restore-password')),
      'pw',
    );
    await t.tap(find.byKey(const ValueKey('net-exchange-restore-confirm')));
    await t.pumpAndSettle();
    expect(
      api.netCalls,
      contains('exchange-restore-peek:/tmp/muninn-exchanges.age'),
    );
    expect(find.text('Replace node identity'), findsOneWidget);
    await t.tap(find.text('Confirm'));
    await t.pumpAndSettle();
    expect(
      api.netCalls,
      contains('exchange-restore:/tmp/muninn-exchanges.age:true'),
    );
  });

  testWidgets('MCP servers and clients list without exposing secrets', (
    t,
  ) async {
    final (api, c) = await pump(t);
    api.mcpServerList = const [
      McpServerInfo(
        id: 'srv-1',
        name: 'docs',
        transport: 'http',
        url: 'https://mcp.example/docs',
        credentials: ['token'],
      ),
    ];
    api.mcpClientList = const [
      McpClientInfo(id: 'cli-1', name: 'editor', role: 'read'),
    ];
    api.mcpToolList = const [
      McpToolInfo(name: 'search', description: 'look up docs', approved: false),
    ];
    await unlock(t);
    await t.tap(find.text('Net'));
    await t.pumpAndSettle();
    await t.tap(find.text('MCP…'));
    await t.pumpAndSettle();
    expect(c.read(netPaneProvider), NetPane.mcp);
    expect(find.text('docs'), findsOneWidget);
    expect(find.textContaining('https://mcp.example/docs'), findsOneWidget);
    expect(find.text('editor'), findsOneWidget);
    expect(find.textContaining('secret'), findsNothing);
    expect(find.textContaining('sk-'), findsNothing);
    await t.tap(find.byKey(const ValueKey('mcp-server-srv-1')));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('mcp-tools-srv-1')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('mcp-tools:srv-1'));
    expect(find.text('search'), findsOneWidget);
    await t.tap(find.byKey(const ValueKey('mcp-tool-search')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('mcp-approve:srv-1:search:true'));
    await t.tap(find.text('Close'));
    await t.pumpAndSettle();
  });

  testWidgets('Peers messages and live list stored conversations', (t) async {
    final (api, _) = await pump(t);
    api.peerMessageList = const [
      PeerMessageInfo(
        id: 'm1',
        peer: 'peer-a',
        peerLabel: 'Alice',
        direction: 'in',
        text: 'hello from the node',
      ),
    ];
    api.liveSessionList = const [
      LiveSessionInfo(
        id: 's1',
        kind: 'board',
        title: 'standup',
        status: 'open',
      ),
    ];
    await unlock(t);
    await t.tap(find.text('Net'));
    await t.pumpAndSettle();
    await t.tap(find.text('Peers, messages & live…'));
    await t.pumpAndSettle();
    expect(find.text('hello from the node'), findsOneWidget);
    expect(find.textContaining('standup'), findsOneWidget);
    await t.tap(find.byKey(const ValueKey('collab-mark-read')));
    await t.pumpAndSettle();
    expect(api.extraCalls, contains('peer-messages-read:'));
  });
}
