/// [MuninnApi] over the generated flutter_rust_bridge bindings (lib/src/rust/).
library;

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';

import 'api.dart';
import 'rust/api/about.dart' as r_about;
import 'rust/api/data.dart' as r_data;
import 'rust/api/help.dart' as r_help;
import 'rust/api/info.dart' as r_info;
import 'rust/api/keys.dart' as r_keys;
import 'rust/api/io.dart' as r_io;
import 'rust/api/net.dart' as r_net;
import 'rust/api/orgs.dart' as r_orgs;
import 'rust/api/prefs.dart' as r_prefs;
import 'rust/api/projects.dart' as r_projects;
import 'rust/api/themes.dart' as r_themes;

class FfiMuninnApi implements MuninnApi {
  /// `null` = `MUNINN_HOME`, else `~/.muninn` (shared with the TUI).
  FfiMuninnApi({this.home});
  final String? home;

  @override
  About about() {
    final a = r_about.about();
    return About(
      name: a.name,
      version: a.version,
      tagline: a.tagline,
      motto: a.motto,
      publisher: a.publisher,
      homepage: a.homepage,
      repository: a.repository,
      license: a.license,
      copyright: a.copyright,
      credits: a.credits,
    );
  }

  @override
  Future<List<String>> listThemes() => r_themes.listThemes(home: home);

  @override
  Future<MuninnTheme> loadTheme(String name, String variant) async {
    final t = await r_themes.loadTheme(
      home: home,
      name: name,
      variant: variant,
    );
    return MuninnTheme(
      name: t.name,
      displayName: t.displayName,
      variant: t.variant,
      roles: {for (final r in t.roles) r.role: r.hex},
    );
  }

  @override
  Future<({String name, String mode})> currentTheme() async {
    final c = await r_themes.currentTheme(home: home);
    return (name: c.name, mode: c.mode);
  }

  @override
  Future<SavedState> savedState() async {
    final s = await r_prefs.savedState(home: home);
    return SavedState(
      theme: s.theme,
      mode: s.mode,
      range: TimeRange(from: s.from?.toInt(), to: s.to?.toInt()),
      dataset: s.dataset,
    );
  }

  @override
  Future<void> saveTheme(String name) =>
      r_prefs.saveTheme(home: home, name: name);
  @override
  Future<void> saveThemeMode(String mode) =>
      r_prefs.saveThemeMode(home: home, mode: mode);
  @override
  Future<void> saveTimeRange(TimeRange r) => r_prefs.saveTimeRange(
    home: home,
    from: r.from == null ? null : PlatformInt64Util.from(r.from!),
    to: r.to == null ? null : PlatformInt64Util.from(r.to!),
  );
  @override
  Future<void> saveDataset(String? key) =>
      r_prefs.saveDataset(home: home, key: key);

  @override
  Future<DataDir> dataDir() async {
    final d = await r_info.dataDirInfo(home: home);
    return DataDir(
      home: d.home,
      database: d.database,
      databaseExists: d.databaseExists,
      keysInitialized: d.keysInitialized,
    );
  }

  @override
  Future<void> unlock(String password) =>
      r_data.unlock(home: home, password: password);

  @override
  void lock() => r_data.lock();

  @override
  Future<Overview> overview() async {
    final o = await r_data.overview();
    return Overview(
      projects: [
        for (final p in o.projects)
          Project(
            id: p.id,
            shortId: p.shortId,
            name: p.name,
            description: p.description,
          ),
      ],
      datasets: [
        for (final d in o.datasets)
          Dataset(
            source: d.source,
            base: d.base,
            quote: d.quote,
            duration: d.duration,
            durationS: d.durationS.toInt(),
            count: d.count.toInt(),
            first: d.first.toInt(),
            last: d.last.toInt(),
          ),
      ],
    );
  }

  @override
  Future<List<ProjectSeries>> projectSeries(String projectId) async => [
    for (final s in await r_data.projectSeries(projectId: projectId))
      ProjectSeries(
        itemId: s.itemId.toInt(),
        source: s.source,
        base: s.base,
        quote: s.quote,
        durationS: s.durationS.toInt(),
        from: s.from?.toInt(),
        to: s.to?.toInt(),
      ),
  ];

  @override
  Future<List<BarPoint>> queryBars(
    Dataset d,
    TimeRange range, {
    int maxPoints = 600,
  }) async => [
    for (final b in await r_data.queryBars(
      source: d.source,
      base: d.base,
      quote: d.quote,
      durationS: PlatformInt64Util.from(d.durationS),
      from: range.from == null ? null : PlatformInt64Util.from(range.from!),
      to: range.to == null ? null : PlatformInt64Util.from(range.to!),
      maxPoints: maxPoints,
    ))
      BarPoint(
        ts: b.ts.toInt(),
        open: b.open,
        high: b.high,
        low: b.low,
        close: b.close,
        volume: b.volume,
      ),
  ];

  @override
  Future<IngestResult> ingestPath(String path, {String? source}) async {
    final o = await r_io.ingestPath(path: path, source: source);
    return IngestResult(
      files: [
        for (final f in o.files)
          IngestFileReport(
            locator: f.locator,
            source: f.source,
            format: f.format,
            bytes: f.bytes.toInt(),
            id: f.id,
            skipped: f.skipped,
            elapsedMs: f.elapsedMs.toInt(),
          ),
      ],
      cancelled: o.cancelled,
    );
  }

  @override
  void cancelIngest() => r_io.cancelIngest();

  @override
  IngestProgress? ingestProgress() {
    final p = r_io.ingestProgress();
    if (p == null) return null;
    return IngestProgress(
      fileIndex: p.fileIndex.toInt(),
      file: p.file,
      overall: p.overall,
      bytesDone: p.bytesDone.toInt(),
      bytesTotal: p.bytesTotal.toInt(),
      cancelled: p.cancelled,
    );
  }

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
    final o = await r_io.exportBars(
      path: path,
      format: format,
      source: source,
      symbol: symbol,
      duration: duration,
      from: from,
      to: to,
    );
    return ExportResult(path: o.path, format: o.format, rows: o.rows.toInt());
  }

  @override
  Future<Project> createProject(String name, {String description = ''}) async {
    final p = await r_projects.createProject(
      name: name,
      description: description,
    );
    return Project(
      id: p.id,
      shortId: p.shortId,
      name: p.name,
      description: p.description,
    );
  }

  @override
  Future<Project> updateProject(
    String id, {
    String? name,
    String? description,
  }) async {
    final p = await r_projects.updateProject(
      id: id,
      name: name,
      description: description,
    );
    return Project(
      id: p.id,
      shortId: p.shortId,
      name: p.name,
      description: p.description,
    );
  }

  @override
  Future<void> removeProject(String id) => r_projects.removeProject(id: id);

  @override
  Future<bool> addProjectSeries(String projectId, Dataset d, TimeRange range) =>
      r_projects.addProjectSeries(
        projectId: projectId,
        source: d.source,
        base: d.base,
        quote: d.quote,
        durationS: PlatformInt64Util.from(d.durationS),
        from: range.from == null ? null : PlatformInt64Util.from(range.from!),
        to: range.to == null ? null : PlatformInt64Util.from(range.to!),
      );

  @override
  Future<void> removeProjectSeries(int itemId) =>
      r_projects.removeProjectSeries(itemId: PlatformInt64Util.from(itemId));

  @override
  Future<NetOverview> netOverview() async {
    final o = await r_net.netOverview(home: home);
    return NetOverview(
      status: NetStatus(
        label: o.status.label,
        peerId: o.status.peerId,
        fingerprint: o.status.fingerprint,
        running: o.status.running,
        pid: o.status.pid?.toInt(),
        startedAt: o.status.startedAt?.toInt(),
        nat: o.status.nat,
        mdns: o.status.mdns,
        relayServer: o.status.relayServer,
        addresses: o.status.addresses,
        reservations: o.status.reservations,
        connected: o.status.connected.toInt(),
        exchangesActive: o.status.exchangesActive.toInt(),
        incoming: o.status.incoming.toInt(),
        shared: o.status.shared.toInt(),
      ),
      peers: [
        for (final p in o.peers)
          NetPeerInfo(
            peerId: p.peerId,
            shortId: p.shortId,
            label: p.label,
            user: p.user,
            caps: p.caps,
            link: p.link,
            source: p.source,
            lastSeen: p.lastSeen.toInt(),
          ),
      ],
    );
  }

  NetPeerInfo _netPeer(r_net.NetPeerInfo p) => NetPeerInfo(
    peerId: p.peerId,
    shortId: p.shortId,
    label: p.label,
    user: p.user,
    caps: p.caps,
    link: p.link,
    source: p.source,
    lastSeen: p.lastSeen.toInt(),
  );

  NetLinkResult _netLink(r_net.NetLinkResult r) => NetLinkResult(
    status: r.status,
    peer: r.peer,
    peerId: r.peerId,
    detail: r.detail,
    warning: r.warning,
  );

  @override
  Future<String> netStart() => r_net.netStart(home: home);

  @override
  Future<String> netStop() => r_net.netStop(home: home);

  @override
  Future<NetLinkResult> netConnect(
    String target, {
    String? label,
    bool hello = false,
  }) async => _netLink(
    await r_net.netConnect(
      home: home,
      target: target,
      label: label,
      hello: hello,
    ),
  );

  @override
  Future<String> netInvite({String? label}) =>
      r_net.netInvite(home: home, label: label);

  @override
  Future<NetInvites> netInvites() async {
    final o = await r_net.netInvites(home: home);
    return NetInvites(
      invites: [
        for (final x in o.invites)
          NetInviteInfo(
            id: x.id,
            shortId: x.shortId,
            title: x.title,
            purpose: x.purpose,
            expiresAt: x.expiresAt?.toInt(),
            createdAt: x.createdAt.toInt(),
            link: x.link,
          ),
      ],
      requests: [
        for (final x in o.requests)
          NetRequestInfo(
            id: x.id,
            shortId: x.shortId,
            title: x.title,
            purpose: x.purpose,
            peerId: x.peerId,
            createdAt: x.createdAt.toInt(),
          ),
      ],
    );
  }

  @override
  Future<NetLinkResult> netAccept(String request) async =>
      _netLink(await r_net.netAccept(home: home, request: request));

  @override
  Future<String> netDecline(String request) =>
      r_net.netDecline(home: home, request: request);

  @override
  Future<String> netDisconnect(String peer) =>
      r_net.netDisconnect(home: home, peer: peer);

  @override
  Future<String> netForget(String peer) =>
      r_net.netForget(home: home, peer: peer);

  @override
  Future<NetRelayInfo> netRelays() async {
    final o = await r_net.netRelays(home: home);
    return NetRelayInfo(
      configured: o.configured,
      seen: [for (final p in o.seen) _netPeer(p)],
      serving: o.serving,
    );
  }

  @override
  Future<String> netRelayAdd(String addr) =>
      r_net.netRelayAdd(home: home, addr: addr);

  @override
  Future<String> netRelayRemove(String relay) =>
      r_net.netRelayRemove(home: home, relay: relay);

  @override
  Future<String> netRelayServe(bool on) =>
      r_net.netRelayServe(home: home, on_: on);

  @override
  Future<List<NetShareInfo>> netShared() async => [
    for (final s in await r_net.netShared(home: home))
      NetShareInfo(
        id: s.id,
        shortId: s.shortId,
        kind: s.kind,
        name: s.name,
        withWhom: s.with_,
      ),
  ];

  @override
  Future<String> netShare(
    String kind,
    String item, {
    List<String> withPeers = const [],
  }) => r_net.netShare(home: home, kind: kind, item: item, with_: withPeers);

  @override
  Future<String> netUnshare(
    String kind,
    String item, {
    List<String> withPeers = const [],
  }) => r_net.netUnshare(home: home, kind: kind, item: item, with_: withPeers);

  @override
  Future<OrgOverview> orgOverview() async =>
      _orgOverview(await r_orgs.orgOverview());

  @override
  Future<OrgInfo> orgCreate(
    String name, {
    String? slug,
    String description = '',
    bool publicOrg = false,
    bool openAccess = false,
  }) async => _orgInfo(
    await r_orgs.orgCreate(
      name: name,
      slug: slug,
      description: description,
      public: publicOrg,
      open: openAccess,
    ),
  );

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
  }) async => _orgInfo(
    await r_orgs.orgEdit(
      org: org,
      name: name,
      slug: slug,
      description: description,
      visibility: visibility,
      access: access,
      hosted: hosted,
      memberAuth: memberAuth,
      gated: gated,
      template: template,
      applyRole: applyRole,
      inviteRole: inviteRole,
    ),
  );

  @override
  Future<String> orgRemove(String org) => r_orgs.orgRemove(org: org);

  @override
  Future<String> orgJoin(String token) =>
      r_orgs.orgJoin(home: home, token: token);

  @override
  Future<String> orgApply(String target, String org, {String note = ''}) =>
      r_orgs.orgApply(home: home, target: target, org: org, note: note);

  @override
  Future<String> orgSync(String org) => r_orgs.orgSync(home: home, org: org);

  @override
  Future<String> orgLeave(String org) => r_orgs.orgLeave(home: home, org: org);

  @override
  Future<OrgRoleInfo> orgRoleCreate(
    String org,
    String name, {
    int rank = 30,
    List<String> perms = const [],
  }) async {
    final r = await r_orgs.orgRoleCreate(
      org: org,
      name: name,
      rank: PlatformInt64Util.from(rank),
      perms: perms,
    );
    return OrgRoleInfo(name: r.name, rank: r.rank.toInt(), perms: r.perms);
  }

  @override
  Future<OrgRoleInfo> orgRoleEdit(
    String org,
    String role, {
    String? name,
    int? rank,
    List<String>? perms,
  }) async {
    final r = await r_orgs.orgRoleEdit(
      org: org,
      role: role,
      name: name,
      rank: rank == null ? null : PlatformInt64Util.from(rank),
      perms: perms,
    );
    return OrgRoleInfo(name: r.name, rank: r.rank.toInt(), perms: r.perms);
  }

  @override
  Future<String> orgRoleRemove(String org, String role) =>
      r_orgs.orgRoleRemove(org: org, role: role);

  @override
  Future<String> orgMemberInvite(String org, {String? to}) =>
      r_orgs.orgMemberInvite(home: home, org: org, to: to);

  @override
  Future<String> orgMemberApprove(String org, String member) =>
      r_orgs.orgMemberApprove(home: home, org: org, member: member);

  @override
  Future<String> orgMemberReject(String org, String member) =>
      r_orgs.orgMemberReject(org: org, member: member);

  @override
  Future<String> orgMemberRole(String org, String member, String role) =>
      r_orgs.orgMemberRole(org: org, member: member, role: role);

  @override
  Future<String> orgMemberKick(
    String org,
    String member, {
    String leftovers = 'keep',
  }) => r_orgs.orgMemberKick(
    home: home,
    org: org,
    member: member,
    leftovers: leftovers,
  );

  @override
  Future<String> orgResourceAdd(String org, String kind, String name) =>
      r_orgs.orgResourceAdd(org: org, kind: kind, name: name);

  @override
  Future<String> orgResourcePublish(String org, String resource) =>
      r_orgs.orgResourcePublish(org: org, resource: resource);

  @override
  Future<String> orgResourceArchive(String org, String resource) =>
      r_orgs.orgResourceArchive(org: org, resource: resource);

  @override
  Future<String> orgResourceRelease(String org, String resource) =>
      r_orgs.orgResourceRelease(org: org, resource: resource);

  @override
  Future<OrgHostStatus> orgHostStatus() async =>
      _orgHost(await r_orgs.orgHostStatus(home: home));

  @override
  Future<OrgHostStatus> orgHostStart() async =>
      _orgHost(await r_orgs.orgHostStart(home: home));

  @override
  Future<String> orgHostStop() => r_orgs.orgHostStop(home: home);

  @override
  Future<String> orgHostAutostart(bool on) =>
      r_orgs.orgHostAutostart(home: home, on_: on);

  @override
  Future<KeysStatus> keysStatus() async {
    final s = await r_keys.keysStatus(home: home);
    return KeysStatus(
      initialized: s.initialized,
      dbId: s.dbId,
      database: s.database,
      serviceVersion: s.serviceVersion.toInt(),
      services: [
        for (final x in s.services)
          ServiceKeyInfo(version: x.version.toInt(), retired: x.retired),
      ],
    );
  }

  @override
  Future<List<HelpTopicInfo>> helpTopics() async => [
    for (final t in await r_help.helpTopics())
      HelpTopicInfo(id: t.id, title: t.title),
  ];

  @override
  Future<HelpTopic> helpTopic(String id) async {
    final t = await r_help.helpTopic(id: id);
    return HelpTopic(id: t.id, title: t.title, body: t.body);
  }

  @override
  Future<List<HelpTopicInfo>> helpSearch(String query) async => [
    for (final t in await r_help.helpSearch(query: query))
      HelpTopicInfo(id: t.id, title: t.title),
  ];

  @override
  Future<String?> helpResolveHref(String from, String href) =>
      r_help.helpResolveHref(from: from, href: href);

  RevealHandle _reveal(dynamic h) => RevealHandle(
    id: h.id.toInt(),
    dbId: h.dbId,
    wordCount: h.wordCount.toInt(),
    shardCount: h.shardCount.toInt(),
    migratedPlainDb: h.migratedPlainDb,
  );

  @override
  Future<RevealHandle> keysInit(String password) async =>
      _reveal(await r_keys.keysInit(home: home, password: password));

  @override
  Future<String> keysRevealWord(int id, int index) => r_keys.keysRevealWord(
    id: PlatformInt64Util.from(id),
    index: PlatformInt64Util.from(index),
  );

  @override
  Future<String> keysRevealShard(int id, int index) => r_keys.keysRevealShard(
    id: PlatformInt64Util.from(id),
    index: PlatformInt64Util.from(index),
  );

  @override
  Future<void> keysRevealDismiss(int id) =>
      r_keys.keysRevealDismiss(id: PlatformInt64Util.from(id));

  @override
  Future<int> keysRotate({
    required String password,
    String? words,
    List<String> shards = const [],
    String? wordsFile,
    String? masterFile,
    String? filePassword,
  }) async {
    final v = await r_keys.keysRotate(
      home: home,
      password: password,
      words: words,
      shards: shards,
      wordsFile: wordsFile,
      masterFile: masterFile,
      filePassword: filePassword,
    );
    return v.toInt();
  }

  @override
  Future<void> keysRekey({
    String? words,
    List<String> shards = const [],
    String? wordsFile,
    String? masterFile,
    String? filePassword,
  }) => r_keys.keysRekey(
    home: home,
    words: words,
    shards: shards,
    wordsFile: wordsFile,
    masterFile: masterFile,
    filePassword: filePassword,
  );

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
  }) => r_keys.keysExport(
    home: home,
    out: out,
    exportPassword: exportPassword,
    only: only,
    confirmCombined: confirmCombined,
    words: words,
    shards: shards,
    wordsFile: wordsFile,
    masterFile: masterFile,
    filePassword: filePassword,
  );

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
    final h = await r_keys.keysRecover(
      home: home,
      newPassword: newPassword,
      printWords: printWords,
      words: words,
      shards: shards,
      wordsFile: wordsFile,
      masterFile: masterFile,
      filePassword: filePassword,
    );
    return h == null ? null : _reveal(h);
  }

  @override
  Future<RevealHandle> keysReissueShards({
    String? words,
    List<String> shards = const [],
    String? wordsFile,
    String? masterFile,
    String? filePassword,
  }) async => _reveal(
    await r_keys.keysReissueShards(
      home: home,
      words: words,
      shards: shards,
      wordsFile: wordsFile,
      masterFile: masterFile,
      filePassword: filePassword,
    ),
  );
}

OrgOverview _orgOverview(r_orgs.OrgOverview o) => OrgOverview(
  mine: [for (final x in o.mine) _orgInfo(x)],
  joined: [
    for (final j in o.joined)
      OrgJoined(
        name: j.name,
        slug: j.slug,
        status: j.status,
        role: j.role,
        node: j.node,
        perms: j.perms,
      ),
  ],
  mirrors: [
    for (final m in o.mirrors)
      OrgMirror(slug: m.slug, fetchedAt: m.fetchedAt.toInt()),
  ],
);

OrgInfo _orgInfo(r_orgs.OrgInfo x) => OrgInfo(
  id: x.id,
  shortId: x.shortId,
  name: x.name,
  slug: x.slug,
  description: x.description,
  visibility: x.visibility,
  access: x.access,
  hosted: x.hosted,
  memberAuth: x.memberAuth,
  gated: x.gated,
  members: x.members.toInt(),
  applyRole: x.applyRole,
  inviteRole: x.inviteRole,
  template: x.template,
  roles: [
    for (final r in x.roles)
      OrgRoleInfo(name: r.name, rank: r.rank.toInt(), perms: r.perms),
  ],
  memberList: [
    for (final m in x.memberList)
      OrgMemberInfo(
        shortId: m.shortId,
        title: m.title,
        role: m.role,
        status: m.status,
        note: m.note,
      ),
  ],
  resources: [
    for (final r in x.resources)
      OrgResourceInfo(
        kind: r.kind,
        name: r.name,
        state: r.state,
        published: r.published,
      ),
  ],
  projects: x.projects,
  endpoints: [
    for (final e in x.endpoints)
      OrgEndpointInfo(
        name: e.name,
        kind: e.kind,
        perm: e.perm,
        anyone: e.anyone,
      ),
  ],
  templates: x.templates,
);

OrgHostStatus _orgHost(r_orgs.OrgHostStatus s) => OrgHostStatus(
  running: s.running,
  ours: s.ours,
  url: s.url,
  addr: s.addr,
  pid: s.pid?.toInt(),
  orgs: s.orgs,
  mirrors: s.mirrors,
  certSource: s.certSource,
  httpAddr: s.httpAddr,
  autostart: s.autostart,
);
