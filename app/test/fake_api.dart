import 'package:muninn/src/api.dart';

const tokyoDark = {
  'bg': '#1a1b26',
  'fg': '#c0caf5',
  'muted': '#565f89',
  'border': '#3b4261',
  'border_focus': '#7aa2f7',
  'selection_bg': '#283457',
  'selection_fg': '#c0caf5',
  'accent': '#bb9af7',
  'header': '#7aa2f7',
  'up': '#9ece6a',
  'down': '#f7768e',
  'warning': '#e0af68',
  'error': '#f7768e',
  'success': '#9ece6a',
  'info': '#7dcfff',
  'chart_1': '#7aa2f7',
  'chart_2': '#bb9af7',
  'chart_3': '#7dcfff',
  'chart_4': '#ff9e64',
  'chart_5': '#9ece6a',
  'chart_6': '#e0af68',
};
const nordLight = {
  'bg': '#eceff4',
  'fg': '#2e3440',
  'header': '#5e81ac',
  'accent': '#b48ead',
  'border': '#d8dee9',
};

class FakeApi implements MuninnApi {
  final loaded = <String>[];
  String? lastPassword;

  @override
  About about() => const About(
    name: 'Muninn',
    version: '2.21.0',
    tagline: 'Local, encrypted workbench for market data with peer-to-peer data workflows.',
    motto: "Odin's raven of memory: it remembers every bar.",
    publisher: 'WeirdBrains',
    homepage: 'https://muninn.weirdbrains.com',
    repository: 'https://github.com/WeirdBrains/muninn',
    license: 'BSD-3-Clause',
    copyright: '© 2026',
    credits: [],
  );

  @override
  Future<List<String>> listThemes() async => ['tokyo-night', 'nord'];

  @override
  Future<MuninnTheme> loadTheme(String name, String variant) async {
    loaded.add('$name/$variant');
    return MuninnTheme(
      name: name,
      displayName: name,
      variant: variant,
      roles: name == 'nord' ? nordLight : tokyoDark,
    );
  }

  @override
  Future<({String name, String mode})> currentTheme() async =>
      (name: 'tokyo-night', mode: 'dark');

  @override
  Future<DataDir> dataDir() async => const DataDir(
    home: '/tmp/muninn-test',
    database: '/tmp/muninn-test/storage/muninn.database',
    databaseExists: true,
    keysInitialized: true,
  );

  SavedState saved = const SavedState(mode: 'dark');
  final saves = <String>[];
  @override
  Future<SavedState> savedState() async => saved;
  @override
  Future<void> saveTheme(String name) async => saves.add('theme=$name');
  @override
  Future<void> saveThemeMode(String mode) async => saves.add('mode=$mode');
  @override
  Future<void> saveTimeRange(TimeRange r) async =>
      saves.add('range=${r.from}..${r.to}');
  @override
  Future<void> saveDataset(String? key) async => saves.add('dataset=$key');

  bool unlocked = false;
  final queries = <TimeRange>[];

  @override
  Future<void> unlock(String password) async {
    lastPassword = password;
    if (password != 'pw') throw 'wrong key or not a muninn database';
    unlocked = true;
  }

  @override
  void lock() => unlocked = false;

  List<Project> projects = const [
    Project(
      id: 'p1',
      shortId: 'abc123',
      name: 'Alpha',
      description: 'test project',
    ),
  ];
  final Map<String, List<ProjectSeries>> seriesByProject = {
    'p1': [
      const ProjectSeries(
        itemId: 1,
        source: 'coinbase',
        base: 'BTC',
        quote: 'USD',
        durationS: 300,
        from: 1704067200000,
      ),
    ],
  };
  int nextProject = 2;
  int nextItemId = 2;
  final createdProjects = <String>[];
  final updatedProjects = <({String id, String? name, String? description})>[];
  final removedProjects = <String>[];
  final addedSeries = <({String projectId, Dataset d, TimeRange range})>[];
  final removedSeries = <int>[];
  Object? projectError;

  @override
  Future<Overview> overview() async => Overview(
    projects: List<Project>.from(projects),
    datasets: const [
      Dataset(
        source: 'coinbase',
        base: 'BTC',
        quote: 'USD',
        duration: '5m',
        durationS: 300,
        count: 288,
        first: 1704067200000,
        last: 1704153300000,
      ),
      Dataset(
        source: 'binance',
        base: 'ETH',
        quote: 'USDT',
        duration: '1h',
        durationS: 3600,
        count: 24,
        first: 1609459200000,
        last: 1609542000000,
      ),
    ],
  );

  @override
  Future<List<ProjectSeries>> projectSeries(String projectId) async =>
      List<ProjectSeries>.from(seriesByProject[projectId] ?? const []);

  @override
  Future<Project> createProject(String name, {String description = ''}) async {
    if (projectError != null) throw projectError!;
    createdProjects.add(name);
    final p = Project(
      id: 'p$nextProject',
      shortId: 'new$nextProject',
      name: name,
      description: description,
    );
    nextProject++;
    projects = [...projects, p];
    seriesByProject[p.id] = [];
    return p;
  }

  @override
  Future<Project> updateProject(
    String id, {
    String? name,
    String? description,
  }) async {
    if (projectError != null) throw projectError!;
    updatedProjects.add((id: id, name: name, description: description));
    final i = projects.indexWhere((p) => p.id == id);
    if (i < 0) throw 'no such project';
    final cur = projects[i];
    final p = Project(
      id: cur.id,
      shortId: cur.shortId,
      name: name ?? cur.name,
      description: description ?? cur.description,
    );
    projects = [...projects]..[i] = p;
    return p;
  }

  @override
  Future<void> removeProject(String id) async {
    if (projectError != null) throw projectError!;
    removedProjects.add(id);
    projects = projects.where((p) => p.id != id).toList();
    seriesByProject.remove(id);
  }

  @override
  Future<bool> addProjectSeries(
    String projectId,
    Dataset d,
    TimeRange range,
  ) async {
    if (projectError != null) throw projectError!;
    addedSeries.add((projectId: projectId, d: d, range: range));
    final list = List<ProjectSeries>.from(
      seriesByProject[projectId] ?? const [],
    );
    if (list.any(
      (s) =>
          s.source == d.source &&
          s.base == d.base &&
          s.quote == d.quote &&
          s.durationS == d.durationS,
    )) {
      return false;
    }
    list.add(
      ProjectSeries(
        itemId: nextItemId++,
        source: d.source,
        base: d.base,
        quote: d.quote,
        durationS: d.durationS,
        from: range.from,
        to: range.to,
      ),
    );
    seriesByProject[projectId] = list;
    return true;
  }

  @override
  Future<void> removeProjectSeries(int itemId) async {
    if (projectError != null) throw projectError!;
    removedSeries.add(itemId);
    for (final id in seriesByProject.keys.toList()) {
      seriesByProject[id] = seriesByProject[id]!
          .where((s) => s.itemId != itemId)
          .toList();
    }
  }

  @override
  Future<List<BarPoint>> queryBars(
    Dataset d,
    TimeRange range, {
    int maxPoints = 600,
  }) async {
    queries.add(range);
    return [
      for (var i = 0; i < 50; i++)
        BarPoint(
          ts: d.first + i * d.durationS * 1000,
          open: 100.0 + i,
          high: 102.0 + i,
          low: 99.0 + i,
          close: i.isEven ? 101.0 + i : 99.5 + i,
          volume: 10.0 + i,
        ),
    ];
  }

  final ingests = <({String path, String? source})>[];
  final exports =
      <
        ({
          String path,
          String format,
          String? source,
          String? symbol,
          String? duration,
          String? from,
          String? to,
        })
      >[];
  IngestResult nextIngest = const IngestResult(
    files: [
      IngestFileReport(
        locator: '/tmp/a.csv',
        source: 'math',
        format: 'csv',
        bytes: 8,
        id: 'abc',
        skipped: false,
        elapsedMs: 12,
      ),
    ],
  );
  ExportResult nextExport = const ExportResult(
    path: '/tmp/out.csv',
    format: 'csv',
    rows: 3,
  );
  bool ingestCancelled = false;
  IngestProgress? progress;
  Object? ingestError;
  Object? exportError;

  @override
  Future<IngestResult> ingestPath(String path, {String? source}) async {
    ingests.add((path: path, source: source));
    if (ingestError != null) throw ingestError!;
    return nextIngest;
  }

  @override
  void cancelIngest() => ingestCancelled = true;

  @override
  IngestProgress? ingestProgress() => progress;

  @override
  Future<ExportResult> exportBars({
    required String path,
    required String format,
    String? source,
    String? symbol,
    String? duration,
    String? from,
    String? to,
  }) async {
    exports.add((
      path: path,
      format: format,
      source: source,
      symbol: symbol,
      duration: duration,
      from: from,
      to: to,
    ));
    if (exportError != null) throw exportError!;
    return nextExport;
  }

  NetOverview net = const NetOverview(
    status: NetStatus(label: 'muninn'),
    peers: [
      NetPeerInfo(
        peerId: '12D3KooWtestpeer',
        shortId: 'estpeer',
        label: 'lab',
        caps: 'muninn',
        source: 'mdns',
        lastSeen: 1704067200000,
      ),
    ],
  );

  @override
  Future<NetOverview> netOverview() async => net;

  final netCalls = <String>[];
  String nextInvite = 'muninn://join/testhook';
  NetInvites invites = const NetInvites();
  NetRelayInfo relays = const NetRelayInfo();
  List<NetShareInfo> shared = [];

  @override
  Future<String> netStart() async {
    netCalls.add('start');
    final s = net.status;
    net = NetOverview(
      status: NetStatus(
        label: s.label,
        peerId: s.peerId ?? '12D3KooWlocal',
        fingerprint: s.fingerprint ?? 'aaaa bbbb cccc dddd',
        running: true,
        pid: 7,
        startedAt: 1704067200000,
        nat: 'public',
        mdns: true,
        addresses: const ['/ip4/1.2.3.4/tcp/4741/p2p/12D3KooWlocal'],
        connected: s.connected,
        exchangesActive: s.exchangesActive,
        incoming: s.incoming,
        shared: s.shared,
      ),
      peers: net.peers,
    );
    return 'node ${net.status.label} running as ${net.status.peerId}';
  }

  @override
  Future<String> netStop() async {
    netCalls.add('stop');
    net = NetOverview(
      status: NetStatus(label: net.status.label),
      peers: net.peers,
    );
    return 'node stopped';
  }

  @override
  Future<NetLinkResult> netConnect(
    String target, {
    String? label,
    bool hello = false,
  }) async {
    netCalls.add('connect:$target:${hello ? "hello" : "full"}');
    if (target.trim().isEmpty) {
      throw 'connect to an address, peer id, name or invite link';
    }
    return NetLinkResult(
      status: hello ? 'hello' : 'pending',
      peer: label ?? target,
      detail: hello ? 'says hello' : 'waiting for them',
    );
  }

  @override
  Future<String> netInvite({String? label}) async {
    netCalls.add('invite:${label ?? ""}');
    final info = NetInviteInfo(
      id: 'inv1',
      shortId: 'inv1',
      title: label ?? '(no peer yet)',
      link: nextInvite,
      purpose: 'node',
    );
    invites = NetInvites(
      invites: [...invites.invites, info],
      requests: invites.requests,
    );
    return nextInvite;
  }

  @override
  Future<NetInvites> netInvites() async => invites;

  @override
  Future<NetLinkResult> netAccept(String request) async {
    netCalls.add('accept:$request');
    invites = NetInvites(
      invites: invites.invites,
      requests: [
        for (final r in invites.requests)
          if (r.id != request) r,
      ],
    );
    return NetLinkResult(
      status: 'connected',
      peer: request,
      detail: 'accepted',
    );
  }

  @override
  Future<String> netDecline(String request) async {
    netCalls.add('decline:$request');
    invites = NetInvites(
      invites: invites.invites,
      requests: [
        for (final r in invites.requests)
          if (r.id != request) r,
      ],
    );
    return 'declined $request';
  }

  @override
  Future<String> netDisconnect(String peer) async {
    netCalls.add('disconnect:$peer');
    return 'disconnected from $peer';
  }

  @override
  Future<String> netForget(String peer) async {
    netCalls.add('forget:$peer');
    net = NetOverview(
      status: net.status,
      peers: [
        for (final p in net.peers)
          if (p.peerId != peer && p.label != peer) p,
      ],
    );
    return 'forgot $peer';
  }

  @override
  Future<NetRelayInfo> netRelays() async => relays;

  @override
  Future<String> netRelayAdd(String addr) async {
    if (!addr.contains('/p2p/')) throw 'a relay address ends in /p2p/PEER_ID';
    netCalls.add('relay-add:$addr');
    if (!relays.configured.contains(addr)) {
      relays = NetRelayInfo(
        configured: [...relays.configured, addr],
        seen: relays.seen,
        serving: relays.serving,
      );
    }
    return 'using relay $addr (from the next node start)';
  }

  @override
  Future<String> netRelayRemove(String relay) async {
    netCalls.add('relay-remove:$relay');
    final next = [
      for (final r in relays.configured)
        if (r != relay && !r.contains(relay)) r,
    ];
    if (next.length == relays.configured.length) {
      throw 'no configured relay matches `$relay`';
    }
    relays = NetRelayInfo(
      configured: next,
      seen: relays.seen,
      serving: relays.serving,
    );
    return 'relay removed';
  }

  @override
  Future<String> netRelayServe(bool on) async {
    netCalls.add('relay-serve:$on');
    relays = NetRelayInfo(
      configured: relays.configured,
      seen: relays.seen,
      serving: on,
    );
    return on
        ? 'this node will relay for others (from the next start)'
        : 'this node will not relay';
  }

  @override
  Future<List<NetShareInfo>> netShared() async => shared;

  @override
  Future<String> netShare(
    String kind,
    String item, {
    List<String> withPeers = const [],
  }) async {
    if (item.trim().isEmpty) throw 'name the item after its kind';
    netCalls.add('share:$kind:$item');
    shared = [
      ...shared,
      NetShareInfo(
        id: 's${shared.length}',
        shortId: 's${shared.length}',
        kind: kind,
        name: item,
        withWhom: withPeers.isEmpty
            ? 'everyone with an exchange'
            : withPeers.join(', '),
      ),
    ];
    return 'sharing $kind $item with ${withPeers.isEmpty ? "every key exchange" : withPeers.join(", ")}';
  }

  @override
  Future<String> netUnshare(
    String kind,
    String item, {
    List<String> withPeers = const [],
  }) async {
    netCalls.add('unshare:$kind:$item');
    shared = [
      for (final s in shared)
        if (!(s.kind == kind && s.name == item)) s,
    ];
    return 'stopped sharing $kind $item';
  }

  OrgOverview orgs = const OrgOverview(
    mine: [
      OrgInfo(
        id: 'o1',
        shortId: 'abc12345',
        name: 'Lab',
        slug: 'lab',
        description: 'we look',
        visibility: 'private',
        access: 'members',
        members: 1,
        applyRole: 'user',
        inviteRole: 'user',
        roles: [
          OrgRoleInfo(name: 'owner', rank: 100, perms: ['*']),
        ],
        memberList: [
          OrgMemberInfo(
            shortId: 'm1',
            title: 'alice',
            role: 'owner',
            status: 'member',
          ),
        ],
      ),
    ],
    joined: [
      OrgJoined(
        name: 'Desk',
        slug: 'desk',
        status: 'member',
        role: 'user',
        node: 'abcdefgh',
        perms: ['read'],
      ),
    ],
    mirrors: [OrgMirror(slug: 'copy', fetchedAt: 1704067200000)],
  );

  @override
  Future<OrgOverview> orgOverview() async => orgs;

  final List<String> orgCalls = [];
  OrgHostStatus host = const OrgHostStatus();

  @override
  Future<OrgInfo> orgCreate(
    String name, {
    String? slug,
    String description = '',
    bool publicOrg = false,
    bool openAccess = false,
  }) async {
    orgCalls.add('create:$name');
    final org = OrgInfo(
      id: 'o${orgs.mine.length + 1}',
      shortId: 'new${orgs.mine.length + 1}',
      name: name,
      slug: slug ?? name.toLowerCase().replaceAll(' ', '-'),
      description: description,
      visibility: publicOrg ? 'public' : 'private',
      access: openAccess ? 'open' : 'members',
      members: 1,
      applyRole: 'user',
      inviteRole: 'user',
      roles: const [
        OrgRoleInfo(name: 'owner', rank: 100, perms: ['*']),
      ],
      memberList: const [
        OrgMemberInfo(
          shortId: 'me',
          title: 'me',
          role: 'owner',
          status: 'member',
        ),
      ],
    );
    orgs = OrgOverview(
      mine: [...orgs.mine, org],
      joined: orgs.joined,
      mirrors: orgs.mirrors,
    );
    return org;
  }

  @override
  Future<OrgInfo> orgEdit(
    String org, {
    String? name,
    String? slug,
    String? description,
    String? visibility,
    String? access,
    bool? hosted,
    bool? memberAuth,
    bool? gated,
    String? template,
    String? applyRole,
    String? inviteRole,
  }) async {
    orgCalls.add('edit:$org');
    OrgInfo? updated;
    orgs = OrgOverview(
      mine: [
        for (final o in orgs.mine)
          if (o.id == org || o.slug == org || o.name == org)
            updated = OrgInfo(
              id: o.id,
              shortId: o.shortId,
              name: name ?? o.name,
              slug: slug ?? o.slug,
              description: description ?? o.description,
              visibility: visibility ?? o.visibility,
              access: access ?? o.access,
              hosted: hosted ?? o.hosted,
              memberAuth: memberAuth ?? o.memberAuth,
              gated: gated ?? o.gated,
              members: o.members,
              applyRole: applyRole ?? o.applyRole,
              inviteRole: inviteRole ?? o.inviteRole,
              template: template ?? o.template,
              roles: o.roles,
              memberList: o.memberList,
              resources: o.resources,
              projects: o.projects,
              endpoints: o.endpoints,
              templates: o.templates,
            )
          else
            o,
      ],
      joined: orgs.joined,
      mirrors: orgs.mirrors,
    );
    return updated ?? (throw 'no organization `$org`');
  }

  @override
  Future<String> orgRemove(String org) async {
    orgCalls.add('remove:$org');
    final had = orgs.mine.where(
      (o) => o.id == org || o.slug == org || o.name == org,
    );
    if (had.isEmpty) throw 'no organization `$org`';
    final name = had.first.name;
    orgs = OrgOverview(
      mine: [
        for (final o in orgs.mine)
          if (!(o.id == org || o.slug == org || o.name == org)) o,
      ],
      joined: orgs.joined,
      mirrors: orgs.mirrors,
    );
    return 'removed $name (its resources stay here)';
  }

  @override
  Future<String> orgJoin(String token) async {
    orgCalls.add('join:$token');
    if (token.trim().isEmpty) throw 'paste an invitation token or link';
    orgs = OrgOverview(
      mine: orgs.mine,
      joined: [
        ...orgs.joined,
        OrgJoined(
          name: 'Joined',
          slug: 'joined',
          status: 'member',
          role: 'user',
          node: 'peerpeer',
        ),
      ],
      mirrors: orgs.mirrors,
    );
    return 'joined Joined as user';
  }

  @override
  Future<String> orgApply(String target, String org, {String note = ''}) async {
    orgCalls.add('apply:$target:$org');
    if (target.trim().isEmpty || org.trim().isEmpty) {
      throw 'apply to NODE and name the organization';
    }
    orgs = OrgOverview(
      mine: orgs.mine,
      joined: [
        ...orgs.joined,
        OrgJoined(
          name: org,
          slug: org.toLowerCase(),
          status: 'applied',
          role: '',
          node: target,
        ),
      ],
      mirrors: orgs.mirrors,
    );
    return 'applied to $org (applied)';
  }

  @override
  Future<String> orgSync(String org) async {
    orgCalls.add('sync:$org');
    return '$org: member as user';
  }

  @override
  Future<String> orgLeave(String org) async {
    orgCalls.add('leave:$org');
    orgs = OrgOverview(
      mine: orgs.mine,
      joined: [
        for (final j in orgs.joined)
          if (j.slug != org && j.name != org) j,
      ],
      mirrors: orgs.mirrors,
    );
    return 'left $org';
  }

  @override
  Future<OrgRoleInfo> orgRoleCreate(
    String org,
    String name, {
    int rank = 30,
    List<String> perms = const [],
  }) async {
    orgCalls.add('role-create:$org:$name');
    return OrgRoleInfo(name: name, rank: rank, perms: perms);
  }

  @override
  Future<OrgRoleInfo> orgRoleEdit(
    String org,
    String role, {
    String? name,
    int? rank,
    List<String>? perms,
  }) async {
    orgCalls.add('role-edit:$org:$role');
    return OrgRoleInfo(
      name: name ?? role,
      rank: rank ?? 30,
      perms: perms ?? const [],
    );
  }

  @override
  Future<String> orgRoleRemove(String org, String role) async {
    orgCalls.add('role-remove:$org:$role');
    return 'removed role $role';
  }

  @override
  Future<String> orgMemberInvite(String org, {String? to}) async {
    orgCalls.add('invite:$org:${to ?? ""}');
    return 'invited guest to $org as user. Invitation token from me: TOKEN';
  }

  @override
  Future<String> orgMemberApprove(String org, String member) async {
    orgCalls.add('approve:$org:$member');
    return 'approved $member as user';
  }

  @override
  Future<String> orgMemberReject(String org, String member) async {
    orgCalls.add('reject:$org:$member');
    return 'rejected $member';
  }

  @override
  Future<String> orgMemberRole(String org, String member, String role) async {
    orgCalls.add('member-role:$org:$member:$role');
    return '$member is now $role';
  }

  @override
  Future<String> orgMemberKick(
    String org,
    String member, {
    String leftovers = 'keep',
  }) async {
    orgCalls.add('kick:$org:$member:$leftovers');
    return 'removed $member';
  }

  @override
  Future<String> orgResourceAdd(String org, String kind, String name) async {
    orgCalls.add('resource-add:$org:$kind:$name');
    return '$kind $name belongs to $org';
  }

  @override
  Future<String> orgResourcePublish(String org, String resource) async {
    orgCalls.add('publish:$org:$resource');
    return '$resource: published members';
  }

  @override
  Future<String> orgResourceArchive(String org, String resource) async {
    orgCalls.add('archive:$org:$resource');
    return '$resource: archived';
  }

  @override
  Future<String> orgResourceRelease(String org, String resource) async {
    orgCalls.add('release:$org:$resource');
    return "$resource is no longer $org's (it stays here)";
  }

  @override
  Future<OrgHostStatus> orgHostStatus() async => host;

  @override
  Future<OrgHostStatus> orgHostStart() async {
    orgCalls.add('host-start');
    host = const OrgHostStatus(
      running: true,
      ours: true,
      url: 'https://127.0.0.1:8443/',
      addr: '127.0.0.1:8443',
      pid: 1,
      orgs: ['lab'],
      certSource: 'self-signed',
    );
    return host;
  }

  @override
  Future<String> orgHostStop() async {
    orgCalls.add('host-stop');
    final was = host.running;
    host = OrgHostStatus(autostart: host.autostart);
    return was ? 'site stopped' : 'the site is not being served';
  }

  @override
  Future<String> orgHostAutostart(bool on) async {
    orgCalls.add('host-autostart:$on');
    host = OrgHostStatus(
      running: host.running,
      ours: host.ours,
      url: host.url,
      addr: host.addr,
      pid: host.pid,
      orgs: host.orgs,
      mirrors: host.mirrors,
      certSource: host.certSource,
      httpAddr: host.httpAddr,
      autostart: on,
    );
    return on
        ? 'the site starts with the node'
        : 'the site starts only when asked';
  }

  KeysStatus keys = const KeysStatus(
    initialized: true,
    dbId: 'db-aaaa-bbbb',
    database: '/tmp/muninn-test/storage/muninn.database',
    serviceVersion: 1,
    services: [ServiceKeyInfo(version: 1, retired: false)],
  );

  @override
  Future<KeysStatus> keysStatus() async => keys;

  static const helpPages = <HelpTopic>[
    HelpTopic(
      id: 'README',
      title: 'Muninn',
      body: '# Muninn\n\nLocal encrypted workbench. See [keybindings](keybindings.md) and the [Flutter app](flutter-app.md).\n',
    ),
    HelpTopic(
      id: 'help',
      title: 'In-app help',
      body: '# In-app help\n\nOpen with `?` or F1. Search titles and bodies.\n',
    ),
    HelpTopic(
      id: 'keybindings',
      title: 'Keybindings',
      body: '# Keybindings\n\nEvery key runs the same named action as its menu item.\n\n| key | action |\n|-----|--------|\n| F5 | refresh |\n| `?`, F1 | help |\n| `T` | time range |\n',
    ),
    HelpTopic(
      id: 'keys',
      title: 'Keys',
      body:
          '# Keys\n\nInitialize, rotate, rekey, export, recover and shards.\n',
    ),
    HelpTopic(
      id: 'flutter-app',
      title: 'Flutter app (desktop and mobile)',
      body: '# Flutter app (desktop and mobile)\n\n## Keybindings\n\nThe app uses the same keys as the TUI for every action it already has.\n\n| key | action |\n|-----|--------|\n| `T` | time range picker |\n| F5 | refresh |\n',
    ),
    HelpTopic(
      id: 'charts',
      title: 'Charts',
      body: '# Charts\n\nPinch to zoom. Layers: candles, volume, grid.\n',
    ),
    HelpTopic(
      id: 'projects',
      title: 'Projects',
      body: '# Projects\n\nCreate, rename and attach series.\n',
    ),
    HelpTopic(
      id: 'network',
      title: 'Network',
      body: '# Network\n\nNode status and peers.\n',
    ),
    HelpTopic(
      id: 'organizations',
      title: 'Organizations',
      body: '# Organizations\n\nList and detail.\n',
    ),
    HelpTopic(
      id: 'sidebar',
      title: 'Sidebar',
      body: '# Sidebar\n\nThe dataset list, like Data in the app.\n',
    ),
  ];

  @override
  Future<List<HelpTopicInfo>> helpTopics() async => [
    for (final t in helpPages) HelpTopicInfo(id: t.id, title: t.title),
  ];

  @override
  Future<HelpTopic> helpTopic(String id) async {
    final want = id.trim().isEmpty || id == 'index' ? 'README' : id;
    return helpPages.firstWhere(
      (t) => t.id == want,
      orElse: () => throw 'unknown help topic `$id`',
    );
  }

  @override
  Future<List<HelpTopicInfo>> helpSearch(String query) async {
    final q = query.trim().toLowerCase();
    final all = [
      for (final t in helpPages) HelpTopicInfo(id: t.id, title: t.title),
    ];
    if (q.isEmpty) return all;
    return [
      for (final t in helpPages)
        if (t.title.toLowerCase().contains(q) ||
            t.id.toLowerCase().contains(q) ||
            t.body.toLowerCase().contains(q))
          HelpTopicInfo(id: t.id, title: t.title),
    ];
  }

  @override
  Future<String?> helpResolveHref(String from, String href) async {
    if (href.contains('://') || href.startsWith('mailto:')) return null;
    final path = href.split('#').first;
    if (path.isEmpty) return from;
    return path.replaceAll('.md', '').split('/').last;
  }

  RevealHandle? reveal;
  String initPassword = '';
  final rotated = <int>[];
  final recovered = <String>[];
  final exported = <String>[];
  int nextReveal = 1;
  static const fakeWords = [
    'alpha',
    'bravo',
    'charlie',
    'delta',
    'echo',
    'foxtrot',
    'golf',
    'hotel',
    'india',
    'juliet',
    'kilo',
    'lima',
    'mike',
    'november',
    'oscar',
    'papa',
    'quebec',
    'romeo',
    'sierra',
    'tango',
    'uniform',
    'victor',
    'whiskey',
    'xray',
  ];
  static const fakeShards = [
    'muninn-shard-v1:aaa:3:1:aa:bbbbbbbb',
    'muninn-shard-v1:aaa:3:2:bb:cccccccc',
    'muninn-shard-v1:aaa:3:3:cc:dddddddd',
    'muninn-shard-v1:aaa:3:4:dd:eeeeeeee',
    'muninn-shard-v1:aaa:3:5:ee:ffffffff',
  ];

  RevealHandle _newReveal({
    String dbId = '',
    int words = 0,
    int shards = 0,
    bool migrated = false,
  }) {
    reveal = RevealHandle(
      id: nextReveal++,
      dbId: dbId,
      wordCount: words,
      shardCount: shards,
      migratedPlainDb: migrated,
    );
    return reveal!;
  }

  @override
  Future<RevealHandle> keysInit(String password) async {
    if (password.isEmpty) throw 'the password must not be empty';
    if (keys.initialized) throw 'already initialized';
    initPassword = password;
    keys = KeysStatus(
      initialized: true,
      dbId: 'db-new',
      database: keys.database,
      serviceVersion: 1,
      services: [const ServiceKeyInfo(version: 1)],
    );
    return _newReveal(dbId: 'db-new', words: 24, shards: 5);
  }

  @override
  Future<String> keysRevealWord(int id, int index) async {
    if (reveal == null || reveal!.id != id) {
      throw 'reveal session expired or dismissed';
    }
    if (index < 1 || index > fakeWords.length) throw 'no word $index';
    return fakeWords[index - 1];
  }

  @override
  Future<String> keysRevealShard(int id, int index) async {
    if (reveal == null || reveal!.id != id) {
      throw 'reveal session expired or dismissed';
    }
    if (index < 1 || index > fakeShards.length) throw 'no shard $index';
    return fakeShards[index - 1];
  }

  @override
  Future<void> keysRevealDismiss(int id) async {
    if (reveal?.id == id) reveal = null;
  }

  @override
  Future<int> keysRotate({
    required String password,
    String? words,
    List<String> shards = const [],
    String? wordsFile,
    String? masterFile,
    String? filePassword,
  }) async {
    if (password.isEmpty) throw 'the password must not be empty';
    keys = KeysStatus(
      initialized: true,
      dbId: keys.dbId,
      database: keys.database,
      serviceVersion: keys.serviceVersion + 1,
      services: [
        ...keys.services.map(
          (s) => ServiceKeyInfo(version: s.version, retired: true),
        ),
        ServiceKeyInfo(version: keys.serviceVersion + 1),
      ],
    );
    rotated.add(keys.serviceVersion);
    return keys.serviceVersion;
  }

  @override
  Future<void> keysRekey({
    String? words,
    List<String> shards = const [],
    String? wordsFile,
    String? masterFile,
    String? filePassword,
  }) async {}

  @override
  Future<List<String>> keysExport({
    required String out,
    required String exportPassword,
    List<String> only = const [],
    bool confirmCombined = false,
    String? words,
    List<String> shards = const [],
    String? wordsFile,
    String? masterFile,
    String? filePassword,
  }) async {
    if (exportPassword.isEmpty) throw 'export password must not be empty';
    final items = only.isEmpty
        ? ['master', 'words', 'service', 'shards']
        : only;
    if ((items.contains('master') || items.contains('words')) &&
        items.contains('shards') &&
        !confirmCombined) {
      throw 'writing the master key/words and shards to one directory defeats the vault/shard split; confirm to continue';
    }
    exported.addAll(items.map((i) => '$out/$i'));
    return [for (final i in items) '$out/$i'];
  }

  @override
  Future<RevealHandle?> keysRecover({
    required String newPassword,
    bool printWords = false,
    String? words,
    List<String> shards = const [],
    String? wordsFile,
    String? masterFile,
    String? filePassword,
  }) async {
    if (newPassword.isEmpty) throw 'the password must not be empty';
    recovered.add(newPassword);
    if (!printWords) return null;
    return _newReveal(words: 24);
  }

  @override
  Future<RevealHandle> keysReissueShards({
    String? words,
    List<String> shards = const [],
    String? wordsFile,
    String? masterFile,
    String? filePassword,
  }) async => _newReveal(shards: 5);
}
