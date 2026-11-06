/// What the UI needs from Muninn. [FfiMuninnApi] calls Rust (muninn-ffi); tests use a fake.
library;

class MuninnTheme {
  const MuninnTheme({
    required this.name,
    required this.displayName,
    required this.variant,
    required this.roles,
  });
  final String name, displayName, variant;
  final Map<String, String> roles;
}

class About {
  const About({
    required this.name,
    required this.version,
    required this.tagline,
    required this.motto,
    required this.publisher,
    required this.homepage,
    required this.repository,
    required this.license,
    required this.copyright,
    required this.credits,
  });
  final String name,
      version,
      tagline,
      motto,
      publisher,
      homepage,
      repository,
      license,
      copyright;
  final List<String> credits;
}

class DataDir {
  const DataDir({
    required this.home,
    required this.database,
    required this.databaseExists,
    required this.keysInitialized,
  });
  final String home, database;
  final bool databaseExists, keysInitialized;
}

class Project {
  const Project({
    this.id = '',
    required this.shortId,
    required this.name,
    required this.description,
  });
  final String id, shortId, name, description;
}

class Dataset {
  const Dataset({
    required this.source,
    required this.base,
    required this.quote,
    required this.duration,
    this.durationS = 300,
    required this.count,
    required this.first,
    required this.last,
  });
  final String source, base, quote, duration;
  final int durationS, count, first, last;
  String get symbol => '$base/$quote';
  String get key => '$source:$symbol:$durationS';

  /// Has bars inside [r] (open ends match everything), like the TUI sidebar.
  bool overlaps(TimeRange r) =>
      (r.from == null || last >= r.from!) && (r.to == null || first <= r.to!);
}

/// The global time range (UTC ms, either end open), as `T` in the TUI.
class TimeRange {
  const TimeRange({this.from, this.to});
  final int? from, to;
  static const all = TimeRange();
  bool get isAll => from == null && to == null;
  TimeRange withFrom(int? v) => TimeRange(from: v, to: to);
  TimeRange withTo(int? v) => TimeRange(from: from, to: v);
  @override
  bool operator ==(Object other) =>
      other is TimeRange && other.from == from && other.to == to;
  @override
  int get hashCode => Object.hash(from, to);
}

class BarPoint {
  const BarPoint({
    required this.ts,
    required this.open,
    required this.high,
    required this.low,
    required this.close,
    required this.volume,
  });
  final int ts;
  final double open, high, low, close, volume;
}

/// A series saved in a project, with the project's range.
class ProjectSeries {
  const ProjectSeries({
    this.itemId = 0,
    required this.source,
    required this.base,
    required this.quote,
    required this.durationS,
    this.from,
    this.to,
  });
  final int itemId, durationS;
  final String source, base, quote;
  final int? from, to;
  String get key => '$source:$base/$quote:$durationS';
}

/// `mnnn net status`: the node and counts (no keys).
class NetStatus {
  const NetStatus({
    required this.label,
    this.peerId,
    this.fingerprint,
    this.running = false,
    this.pid,
    this.startedAt,
    this.nat = '',
    this.mdns = false,
    this.relayServer = false,
    this.addresses = const [],
    this.reservations = const [],
    this.connected = 0,
    this.exchangesActive = 0,
    this.incoming = 0,
    this.shared = 0,
  });
  final String label, nat;
  final String? peerId, fingerprint;
  final bool running, mdns, relayServer;
  final int? pid, startedAt;
  final List<String> addresses, reservations;
  final int connected, exchangesActive, incoming, shared;
}

/// `mnnn net peers`: a known peer (public facts only).
class NetPeerInfo {
  const NetPeerInfo({
    required this.peerId,
    required this.shortId,
    this.label = '',
    this.user = '',
    this.caps = '',
    this.link = '-',
    this.source = '',
    this.lastSeen = 0,
  });
  final String peerId, shortId, label, user, caps, link, source;
  final int lastSeen;
}

class NetOverview {
  const NetOverview({required this.status, this.peers = const []});
  final NetStatus status;
  final List<NetPeerInfo> peers;
}

/// Result of connect / accept / hello. No keys.
class NetLinkResult {
  const NetLinkResult({
    required this.status,
    this.peer = '',
    this.peerId,
    this.detail = '',
    this.warning,
  });
  final String status, peer, detail;
  final String? peerId, warning;
}

/// An invite this node made. The link holds no secret.
class NetInviteInfo {
  const NetInviteInfo({
    required this.id,
    required this.shortId,
    this.title = '',
    this.purpose = '',
    this.expiresAt,
    this.createdAt = 0,
    this.link = '',
  });
  final String id, shortId, title, purpose, link;
  final int? expiresAt;
  final int createdAt;
}

class NetRequestInfo {
  const NetRequestInfo({
    required this.id,
    required this.shortId,
    this.title = '',
    this.purpose = '',
    this.peerId,
    this.createdAt = 0,
  });
  final String id, shortId, title, purpose;
  final String? peerId;
  final int createdAt;
}

class NetInvites {
  const NetInvites({this.invites = const [], this.requests = const []});
  final List<NetInviteInfo> invites;
  final List<NetRequestInfo> requests;
}

class NetRelayInfo {
  const NetRelayInfo({
    this.configured = const [],
    this.seen = const [],
    this.serving = false,
  });
  final List<String> configured;
  final List<NetPeerInfo> seen;
  final bool serving;
}

class NetShareInfo {
  const NetShareInfo({
    required this.id,
    required this.shortId,
    required this.kind,
    required this.name,
    this.withWhom = '',
  });
  final String id, shortId, kind, name, withWhom;
}

class Overview {
  const Overview({required this.projects, required this.datasets});
  final List<Project> projects;
  final List<Dataset> datasets;
}

/// What the app restores on launch from preferences.toml (shared with the TUI).
class SavedState {
  const SavedState({
    this.theme = 'tokyo-night',
    this.mode = 'system',
    this.range = TimeRange.all,
    this.dataset,
  });
  final String theme, mode;
  final TimeRange range;
  final String? dataset;
}

/// One file stored by Import (as it arrived; no bars are made).
class IngestFileReport {
  const IngestFileReport({
    required this.locator,
    required this.source,
    required this.format,
    required this.bytes,
    required this.id,
    required this.skipped,
    required this.elapsedMs,
  });
  final String locator, source, format, id;
  final int bytes, elapsedMs;
  final bool skipped;
}

class IngestResult {
  const IngestResult({required this.files, this.cancelled = false});
  final List<IngestFileReport> files;
  final bool cancelled;
}

/// Live import progress, polled while [MuninnApi.ingestPath] runs.
class IngestProgress {
  const IngestProgress({
    required this.fileIndex,
    required this.file,
    required this.overall,
    required this.bytesDone,
    required this.bytesTotal,
    this.cancelled = false,
  });
  final int fileIndex, bytesDone, bytesTotal;
  final double file, overall;
  final bool cancelled;
}

class ExportResult {
  const ExportResult({
    required this.path,
    required this.format,
    required this.rows,
  });
  final String path, format;
  final int rows;
}

/// An organization this node owns (`mnnn org list` / `mnnn org show`).
class OrgInfo {
  const OrgInfo({
    required this.id,
    required this.shortId,
    required this.name,
    required this.slug,
    this.description = '',
    this.visibility = 'private',
    this.access = 'members',
    this.hosted = false,
    this.memberAuth = false,
    this.gated = false,
    this.members = 0,
    this.applyRole = '-',
    this.inviteRole = '-',
    this.template = 'default',
    this.roles = const [],
    this.memberList = const [],
    this.resources = const [],
    this.projects = const [],
    this.endpoints = const [],
    this.templates = const [],
  });
  final String id;
  final String shortId;
  final String name;
  final String slug;
  final String description;
  final String visibility;
  final String access;
  final String applyRole;
  final String inviteRole;
  final String template;
  final bool hosted;
  final bool memberAuth;
  final bool gated;
  final int members;
  final List<OrgRoleInfo> roles;
  final List<OrgMemberInfo> memberList;
  final List<OrgResourceInfo> resources;
  final List<String> projects;
  final List<String> templates;
  final List<OrgEndpointInfo> endpoints;
}

class OrgRoleInfo {
  const OrgRoleInfo({required this.name, this.rank = 0, this.perms = const []});
  final String name;
  final int rank;
  final List<String> perms;
}

class OrgMemberInfo {
  const OrgMemberInfo({
    required this.shortId,
    required this.title,
    this.role = '-',
    this.status = '',
    this.note = '',
  });
  final String shortId;
  final String title;
  final String role;
  final String status;
  final String note;
}

class OrgResourceInfo {
  const OrgResourceInfo({
    required this.kind,
    required this.name,
    this.state = '',
    this.published = '',
  });
  final String kind;
  final String name;
  final String state;
  final String published;
}

class OrgEndpointInfo {
  const OrgEndpointInfo({
    required this.name,
    required this.kind,
    this.perm = '',
    this.anyone = false,
  });
  final String name;
  final String kind;
  final String perm;
  final bool anyone;
}

/// An org on another node that this node applied to or belongs to.
class OrgJoined {
  const OrgJoined({
    required this.name,
    required this.slug,
    this.status = '',
    this.role = '',
    this.node = '',
    this.perms = const [],
  });
  final String name;
  final String slug;
  final String status;
  final String role;
  final String node;
  final List<String> perms;
}

/// A public copy of another node's org (no snapshot).
class OrgMirror {
  const OrgMirror({required this.slug, this.fetchedAt = 0});
  final String slug;
  final int fetchedAt;
}

class OrgOverview {
  const OrgOverview({
    this.mine = const [],
    this.joined = const [],
    this.mirrors = const [],
  });
  final List<OrgInfo> mine;
  final List<OrgJoined> joined;
  final List<OrgMirror> mirrors;
}

/// What `mnnn org host status` shows. No certificate private key.
class OrgHostStatus {
  const OrgHostStatus({
    this.running = false,
    this.ours = false,
    this.url = '',
    this.addr = '',
    this.pid,
    this.orgs = const [],
    this.mirrors = const [],
    this.certSource = '',
    this.httpAddr,
    this.autostart = false,
  });
  final bool running;
  final bool ours;
  final String url;
  final String addr;
  final int? pid;
  final List<String> orgs;
  final List<String> mirrors;
  final String certSource;
  final String? httpAddr;
  final bool autostart;
}

/// What `mnnn keys status` prints. No master key, words, shards or wrapped keys.
class ServiceKeyInfo {
  const ServiceKeyInfo({required this.version, this.retired = false});
  final int version;
  final bool retired;
}

class KeysStatus {
  const KeysStatus({
    this.initialized = false,
    this.dbId = '',
    this.database = '',
    this.serviceVersion = 0,
    this.services = const [],
  });
  final bool initialized;
  final String dbId;
  final String database;
  final int serviceVersion;
  final List<ServiceKeyInfo> services;
}

abstract class MuninnApi {
  About about();
  Future<List<String>> listThemes();
  Future<MuninnTheme> loadTheme(String name, String variant);
  Future<({String name, String mode})> currentTheme();
  Future<DataDir> dataDir();

  /// The saved theme, mode, time range and dataset (read-only; never creates the file).
  Future<SavedState> savedState();

  /// Like `mnnn theme set` / `mnnn theme mode`: written to preferences.toml.
  Future<void> saveTheme(String name);
  Future<void> saveThemeMode(String mode);

  /// The TUI's saved time range (`[tui.sidebar.filters]`).
  Future<void> saveTimeRange(TimeRange range);
  Future<void> saveDataset(String? key);

  /// Unlock with the Muninn password; the key stays in Rust. Throws on a wrong password.
  Future<void> unlock(String password);
  void lock();

  /// Dashboard reads: the store is opened without migrations or writes.
  Future<Overview> overview();
  Future<List<ProjectSeries>> projectSeries(String projectId);

  /// Bars of one dataset in [range], downsampled in Rust to at most [maxPoints].
  Future<List<BarPoint>> queryBars(
    Dataset d,
    TimeRange range, {
    int maxPoints = 600,
  });

  /// Store each data file at [path] (file or folder) byte for byte. Opens the store for writes.
  Future<IngestResult> ingestPath(String path, {String? source});

  /// Ask a running import to stop (finished files stay; the current file is rolled back).
  void cancelIngest();

  /// Snapshot of a running import, or null.
  IngestProgress? ingestProgress();

  /// Write matching bars to [path] as `csv`, `parquet` or `json`. Opens the store for writes.
  Future<ExportResult> exportBars({
    required String path,
    required String format,
    String? source,
    String? symbol,
    String? duration,
    String? from,
    String? to,
  });

  /// `mnnn project create`. Opens the store for writes.
  Future<Project> createProject(String name, {String description = ''});

  /// `mnnn project edit` (rename and/or description).
  Future<Project> updateProject(String id, {String? name, String? description});

  /// `mnnn project remove`. Journals stay as standalone journals.
  Future<void> removeProject(String id);

  /// Add [d] (with [range]) as a project result (`a` in the TUI). `true` if newly added.
  Future<bool> addProjectSeries(String projectId, Dataset d, TimeRange range);

  /// Take a series result out of a project (`mnnn project drop`). The series stays.
  Future<void> removeProjectSeries(int itemId);

  /// `mnnn net status` and `mnnn net peers`. The store is opened read-only.
  Future<NetOverview> netOverview();

  /// `mnnn net start`: run the node in this process. The node key stays in Rust.
  Future<String> netStart();

  /// `mnnn net stop`, or ask another process to stop.
  Future<String> netStop();

  /// `mnnn net connect` (address, peer id, name or invite link). [hello] only says hello.
  Future<NetLinkResult> netConnect(
    String target, {
    String? label,
    bool hello = false,
  });

  /// `mnnn net invite`: a public link (no secret).
  Future<String> netInvite({String? label});
  Future<NetInvites> netInvites();
  Future<NetLinkResult> netAccept(String request);
  Future<String> netDecline(String request);
  Future<String> netDisconnect(String peer);
  Future<String> netForget(String peer);
  Future<NetRelayInfo> netRelays();
  Future<String> netRelayAdd(String addr);
  Future<String> netRelayRemove(String relay);
  Future<String> netRelayServe(bool on);
  Future<List<NetShareInfo>> netShared();
  Future<String> netShare(
    String kind,
    String item, {
    List<String> withPeers = const [],
  });
  Future<String> netUnshare(
    String kind,
    String item, {
    List<String> withPeers = const [],
  });

  /// `mnnn org list` and the fields `mnnn org show` prints for orgs this node owns.
  Future<OrgOverview> orgOverview();

  /// `mnnn org create` / TUI New organization.
  Future<OrgInfo> orgCreate(
    String name, {
    String? slug,
    String description = '',
    bool publicOrg = false,
    bool openAccess = false,
  });

  /// `mnnn org edit` / TUI rename and settings.
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
  });

  /// `mnnn org remove`. Resources stay on this node.
  Future<String> orgRemove(String org);

  Future<String> orgJoin(String token);
  Future<String> orgApply(String target, String org, {String note = ''});
  Future<String> orgSync(String org);
  Future<String> orgLeave(String org);

  Future<OrgRoleInfo> orgRoleCreate(
    String org,
    String name, {
    int rank = 30,
    List<String> perms = const [],
  });
  Future<OrgRoleInfo> orgRoleEdit(
    String org,
    String role, {
    String? name,
    int? rank,
    List<String>? perms,
  });
  Future<String> orgRoleRemove(String org, String role);

  Future<String> orgMemberInvite(String org, {String? to});
  Future<String> orgMemberApprove(String org, String member);
  Future<String> orgMemberReject(String org, String member);
  Future<String> orgMemberRole(String org, String member, String role);
  Future<String> orgMemberKick(
    String org,
    String member, {
    String leftovers = 'keep',
  });

  Future<String> orgResourceAdd(String org, String kind, String name);
  Future<String> orgResourcePublish(String org, String resource);
  Future<String> orgResourceArchive(String org, String resource);
  Future<String> orgResourceRelease(String org, String resource);

  Future<OrgHostStatus> orgHostStatus();
  Future<OrgHostStatus> orgHostStart();
  Future<String> orgHostStop();
  Future<String> orgHostAutostart(bool on);

  /// `mnnn keys status`: database id, path and service-key versions. No key material.
  Future<KeysStatus> keysStatus();

  /// Embedded docs (`mnnn help` / the TUI help viewer).
  Future<List<HelpTopicInfo>> helpTopics();
  Future<HelpTopic> helpTopic(String id);
  Future<List<HelpTopicInfo>> helpSearch(String query);
  Future<String?> helpResolveHref(String from, String href);

  /// `mnnn keys init`. Secrets stay in Rust; [RevealHandle] is a session id and counts.
  Future<RevealHandle> keysInit(String password);
  Future<String> keysRevealWord(int id, int index);
  Future<String> keysRevealShard(int id, int index);
  Future<void> keysRevealDismiss(int id);
  Future<int> keysRotate({
    required String password,
    String? words,
    List<String> shards = const [],
    String? wordsFile,
    String? masterFile,
    String? filePassword,
  });
  Future<void> keysRekey({
    String? words,
    List<String> shards = const [],
    String? wordsFile,
    String? masterFile,
    String? filePassword,
  });
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
  });
  Future<RevealHandle?> keysRecover({
    required String newPassword,
    bool printWords = false,
    String? words,
    List<String> shards = const [],
    String? wordsFile,
    String? masterFile,
    String? filePassword,
  });
  Future<RevealHandle> keysReissueShards({
    String? words,
    List<String> shards = const [],
    String? wordsFile,
    String? masterFile,
    String? filePassword,
  });
}

class HelpTopicInfo {
  const HelpTopicInfo({required this.id, required this.title});
  final String id;
  final String title;
}

class HelpTopic {
  const HelpTopic({required this.id, required this.title, required this.body});
  final String id;
  final String title;
  final String body;
}

/// A one-time Rust-held reveal of recovery words and/or shards. No secrets in this handle.
class RevealHandle {
  const RevealHandle({
    required this.id,
    this.dbId = '',
    this.wordCount = 0,
    this.shardCount = 0,
    this.migratedPlainDb = false,
  });
  final int id;
  final String dbId;
  final int wordCount;
  final int shardCount;
  final bool migratedPlainDb;
}
