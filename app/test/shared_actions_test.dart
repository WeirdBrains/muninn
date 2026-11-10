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
  testWidgets('unlock offers key recovery before the database opens', (
    t,
  ) async {
    await pump(t);
    expect(find.byKey(const ValueKey('unlock-recover-words')), findsOneWidget);
    expect(find.byKey(const ValueKey('unlock-recover-shards')), findsOneWidget);
    expect(find.byKey(const ValueKey('unlock-shard-backup')), findsOneWidget);
    await t.tap(find.byKey(const ValueKey('unlock-shard-backup')));
    await t.pumpAndSettle();
    expect(find.byKey(const ValueKey('shard-restore-go')), findsOneWidget);
  });

  testWidgets('MCP add, exchange offer and live send are on the net panes', (
    t,
  ) async {
    final (api, c) = await pump(t);
    await unlock(t);
    c.read(sectionProvider.notifier).state = Section.net;
    c.read(netPaneProvider.notifier).state = NetPane.mcp;
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('mcp-add-server-open')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('mcp-server-name')), 'docs');
    await t.enterText(
      find.byKey(const ValueKey('mcp-server-url')),
      'https://example.com/mcp',
    );
    await t.tap(find.byKey(const ValueKey('mcp-add-server')));
    await t.pumpAndSettle();
    expect(api.lastMcpAdd, 'docs');
    expect(find.textContaining('added MCP server docs'), findsOneWidget);

    await t.tap(find.byKey(const ValueKey('mcp-add-client-open')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('mcp-client-name')), 'editor');
    await t.tap(find.byKey(const ValueKey('mcp-add-client')));
    await t.pumpAndSettle();
    expect(find.byKey(const ValueKey('once-secret')), findsOneWidget);
    expect(find.textContaining('client-token-editor'), findsOneWidget);
    await t.tap(find.text('Done'));
    await t.pumpAndSettle();

    c.read(netPaneProvider.notifier).state = NetPane.exchanges;
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('net-exchange-offer-open')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('net-offer-label')), 'ben');
    await t.tap(find.byKey(const ValueKey('net-exchange-offer')));
    await t.pumpAndSettle();
    expect(api.lastOffer, 'ben::0');
    expect(find.textContaining('offer-token'), findsOneWidget);
    await t.tap(find.text('Done').last);
    await t.pumpAndSettle();

    c.read(netPaneProvider.notifier).state = NetPane.collab;
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('collab-send-open')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('collab-peer')), 'ben');
    await t.enterText(find.byKey(const ValueKey('collab-text')), 'hello');
    await t.tap(find.byKey(const ValueKey('collab-send')));
    await t.pumpAndSettle();
    expect(api.lastCollab, 'send:ben');
  });

  testWidgets('plugin install and piping a code result', (t) async {
    final (api, c) = await pump(t);
    await unlock(t);
    await t.tap(find.text('Plug'));
    await t.pumpAndSettle();
    await t.tap(find.text('Install plugin…'));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('plugin-what')), 'alpha');
    await t.tap(find.byKey(const ValueKey('plugin-install')));
    await t.pumpAndSettle();
    expect(api.lastPluginInstall, 'parser:alpha');

    api.codeList = [
      const CodeSpaceInfo(id: 'c1', shortId: 'c1', name: 'alpha'),
    ];
    c.read(sectionProvider.notifier).state = Section.projects;
    c.read(projectPaneProvider.notifier).state = ProjectPane.code;
    c.invalidate(codeSpacesProvider);
    await t.pumpAndSettle();
    await t.tap(find.byType(PopupMenuButton<String>).first);
    await t.pumpAndSettle();
    await t.tap(find.text('Pipe result…'));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('pipe-journal-notes')));
    await t.pumpAndSettle();
    expect(api.lastPipe, 'c1>journal:notes');
  });

  testWidgets('announce is on the orgs host bar', (t) async {
    final (api, c) = await pump(t);
    await unlock(t);
    c.read(sectionProvider.notifier).state = Section.orgs;
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('org-announce-open')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('org-announce-key')), 'lab');
    await t.tap(find.byKey(const ValueKey('org-announce')));
    await t.pumpAndSettle();
    expect(api.lastAnnounce, 'lab:false');
    expect(find.textContaining('lab announced'), findsOneWidget);
  });
}
