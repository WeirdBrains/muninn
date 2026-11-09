/// What the UI needs from Muninn. [FfiMuninnApi] calls Rust (muninn-ffi); tests use a fake.
library;

import 'dart:typed_data';

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

/// An ingest source (`mnnn sources list` / TUI Data ▸ Sources…).
class SourceInfo {
  const SourceInfo({
    required this.id,
    required this.name,
    required this.kind,
    this.locator,
    this.createdAt = 0,
    this.items = 0,
    this.bytes = 0,
  });
  final String id, name, kind;
  final String? locator;
  final int createdAt, items, bytes;
}

/// One stored item under a source (`mnnn sources data`), without its bytes.
class SourceDataItem {
  const SourceDataItem({
    required this.id,
    required this.name,
    this.format = '',
    this.locator = '',
    this.size = 0,
    this.contentHash = '',
    this.receivedAt = 0,
  });
  final String id, name, format, locator, contentHash;
  final int size, receivedAt;
}

/// A known pair (`mnnn symbols list` / TUI Data ▸ Symbols…).
class SymbolInfo {
  const SymbolInfo({
    required this.symbol,
    required this.base,
    required this.quote,
  });
  final String symbol, base, quote;
}

/// `alias A -> C` from `mnnn symbols list`.
class AliasInfo {
  const AliasInfo({required this.alias, required this.canonical});
  final String alias, canonical;
}

/// Any item saved in a project (`mnnn project show`).
class ProjectItemInfo {
  const ProjectItemInfo({
    required this.itemId,
    required this.kind,
    required this.name,
    this.detail = '',
    this.refId = '',
    this.source,
    this.base,
    this.quote,
    this.durationS,
    this.from,
    this.to,
  });
  final int itemId;
  final String kind, name, detail, refId;
  final String? source, base, quote;
  final int? durationS, from, to;
  bool get isResult => kind == 'result';
}

class JournalInfo {
  const JournalInfo({
    required this.id,
    required this.shortId,
    required this.name,
    this.projectId,
    this.projectName = '',
    this.format = 'markdown',
    this.entries = 0,
    this.forkedFrom,
  });
  final String id, shortId, name, format, projectName;
  final String? projectId, forkedFrom;
  final int entries;
}

class JournalEntryInfo {
  const JournalEntryInfo({
    required this.id,
    required this.shortId,
    required this.journalId,
    this.title = '',
    this.heading = '',
    this.body = '',
    this.format = 'markdown',
    this.position = 0,
  });
  final String id, shortId, journalId, title, heading, body, format;
  final int position;
}

class JournalNoteInfo {
  const JournalNoteInfo({
    required this.id,
    required this.shortId,
    required this.body,
  });
  final String id, shortId, body;
}

class JournalAnnotationInfo {
  const JournalAnnotationInfo({
    required this.id,
    required this.shortId,
    required this.journalId,
    this.entryId,
    required this.kind,
    this.start,
    this.end,
    this.quote,
    this.body = '',
    this.color,
    this.detached = false,
  });
  final String id, shortId, journalId, kind, body;
  final String? entryId, quote, color;
  final int? start, end;
  final bool detached;
}

class JournalImageInfo {
  const JournalImageInfo({
    required this.id,
    required this.shortId,
    required this.entryId,
    required this.name,
    this.mime = '',
    this.size = 0,
  });
  final String id, shortId, entryId, name, mime;
  final int size;
}

class JournalExportInfo {
  const JournalExportInfo({
    required this.path,
    this.files = 1,
    this.message = '',
  });
  final String path, message;
  final int files;
}

class SearchInfo {
  const SearchInfo({
    required this.id,
    required this.shortId,
    required this.name,
    this.target = 'series',
    this.query = '',
    this.describe = '',
    this.matches = 0,
    this.forkedFrom,
    this.sources = const [],
    this.symbols = const [],
    this.periods = const [],
    this.years = const [],
    this.from,
    this.to,
    this.priceMetric = 'close',
    this.priceMin,
    this.priceMax,
    this.sort = 'year',
    this.desc = false,
    this.model,
    this.conditions = const [],
  });
  final String id, shortId, name, target, query, describe;
  final int matches;
  final String? forkedFrom;
  final List<String> sources, symbols, periods, conditions;
  final List<int> years;
  final int? from, to;
  final String priceMetric, sort;
  final double? priceMin, priceMax;
  final bool desc;
  final String? model;
}

class HistoryRowInfo {
  const HistoryRowInfo({
    required this.id,
    this.kind = '',
    this.summary = '',
    this.depth = 0,
    this.head = false,
    this.current = false,
    this.fork = false,
    this.createdAt = 0,
  });
  final int id, depth, createdAt;
  final String kind, summary;
  final bool head, current, fork;
}

class HistoryMoveInfo {
  const HistoryMoveInfo({
    this.from,
    this.to,
    this.theme = '',
    this.mode = '',
    this.rangeFrom,
    this.rangeTo,
  });
  final int? from, to, rangeFrom, rangeTo;
  final String theme, mode;
}

class AuthorInfo {
  const AuthorInfo({
    required this.id,
    required this.shortId,
    required this.name,
    this.email = '',
    this.details = const [],
    this.current = false,
  });
  final String id, shortId, name, email;
  final List<String> details;
  final bool current;
}

class FoundEntryInfo {
  const FoundEntryInfo({
    required this.journalId,
    required this.journalName,
    required this.entryId,
    this.heading = '',
    this.title = '',
    this.createdAt = 0,
  });
  final String journalId, journalName, entryId, heading, title;
  final int createdAt;
}

class FoundRecordInfo {
  const FoundRecordInfo({
    required this.modelId,
    required this.modelName,
    required this.recordId,
    this.brief = '',
    this.createdAt = 0,
  });
  final String modelId, modelName, recordId, brief;
  final int createdAt;
}

class DesignFieldInfo {
  const DesignFieldInfo({
    required this.id,
    required this.name,
    this.spec = '',
    this.ty = '',
    this.required = false,
    this.unique = false,
    this.indexed = false,
    this.defaultValue = '',
    this.choices = const [],
    this.about = '',
    this.mark = ' ',
    this.dropped = false,
    this.builtin = false,
  });
  final String id, name, spec, ty, defaultValue, about, mark;
  final bool required, unique, indexed, dropped, builtin;
  final List<String> choices;
}

class ModelPlanInfo {
  const ModelPlanInfo({
    this.from = 0,
    this.to = 0,
    this.rows = 0,
    this.lost = 0,
    this.destructive = false,
    this.blocked = const [],
    this.lines = const [],
  });
  final int from, to, rows, lost;
  final bool destructive;
  final List<String> blocked, lines;
}

class ModelTemplateInfo {
  const ModelTemplateInfo({
    required this.name,
    required this.source,
    this.description = '',
    this.fieldCount = 0,
    this.ruleCount = 0,
    this.recordCount = 0,
  });
  final String name, source, description;
  final int fieldCount, ruleCount, recordCount;
}

class ModelImportPlanInfo {
  const ModelImportPlanInfo({
    this.templateName = '',
    this.model = '',
    this.merge = false,
    this.clash = false,
    this.suggestedName = '',
    this.from = 0,
    this.to = 0,
    this.records = 0,
    this.rules = 0,
    this.lost = 0,
    this.destructive = false,
    this.blocked = const [],
    this.kept = const [],
    this.lines = const [],
    this.summary = '',
  });
  final String templateName, model, suggestedName, summary;
  final bool merge, clash, destructive;
  final int from, to, records, rules, lost;
  final List<String> blocked, kept, lines;
}

class ModelVersionInfo {
  const ModelVersionInfo({
    required this.version,
    this.createdAt = 0,
    this.note = '',
    this.author = '',
    this.fieldCount = 0,
    this.current = false,
    this.steps = const [],
  });
  final int version, createdAt, fieldCount;
  final String note, author;
  final bool current;
  final List<String> steps;
}

class ModelInfo {
  const ModelInfo({
    required this.id,
    required this.shortId,
    required this.name,
    this.description = '',
    this.builtin = false,
    this.version = 0,
    this.isBars = false,
    this.fields = const [],
    this.recordCount = 0,
    this.hasDraft = false,
    this.status = '',
    this.designFields = const [],
  });
  final String id, shortId, name, description, status;
  final bool builtin, isBars, hasDraft;
  final int version, recordCount;
  final List<String> fields;
  final List<DesignFieldInfo> designFields;
}

class RecordField {
  const RecordField({required this.name, this.value = ''});
  final String name, value;
}

class RecordInfo {
  const RecordInfo({
    required this.id,
    required this.shortId,
    required this.modelId,
    this.version = 0,
    this.createdAt = 0,
    this.updatedAt = 0,
    this.fields = const [],
  });
  final String id, shortId, modelId;
  final int version, createdAt, updatedAt;
  final List<RecordField> fields;
}

class PipeFrameInfo {
  const PipeFrameInfo({
    required this.key,
    this.columns = const [],
    this.cells = const [],
    this.rowCount = 0,
  });
  final String key;
  final List<String> columns, cells;
  final int rowCount;
}

class ReportExportInfo {
  const ReportExportInfo({
    required this.path,
    this.files = 1,
    this.message = '',
  });
  final String path, message;
  final int files;
}

class ReportInfo {
  const ReportInfo({
    required this.id,
    required this.shortId,
    required this.name,
    this.description = '',
    this.sections = 0,
  });
  final String id, shortId, name, description;
  final int sections;
}

class ReportSectionInfo {
  const ReportSectionInfo({
    required this.id,
    required this.shortId,
    required this.reportId,
    this.position = 0,
    this.kind = 'text',
    this.title = '',
    this.heading = '',
    this.body = '',
    this.format = 'markdown',
  });
  final String id, shortId, reportId, kind, title, heading, body, format;
  final int position;
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

/// `mnnn code list` / TUI Code spaces…. [source] is the text the user wrote.
class CodeSpaceInfo {
  const CodeSpaceInfo({
    required this.id,
    required this.shortId,
    required this.name,
    this.description = '',
    this.lang = 'math',
    this.source = '',
    this.input = 'none',
    this.scope = 'user',
    this.approved = false,
    this.trustHash = '',
    this.lastStatus = '',
    this.lastError = '',
    this.lastAt = 0,
    this.lastMs = 0,
    this.lastWrites = 0,
  });
  final String id, shortId, name, description, lang, source, input, scope;
  final bool approved;
  final String trustHash, lastStatus, lastError;
  final int lastAt, lastMs, lastWrites;
}

class CodeParserInfo {
  const CodeParserInfo({
    required this.name,
    this.about = '',
    this.installed = false,
  });
  final String name, about;
  final bool installed;
}

class CodeRunResult {
  const CodeRunResult({
    this.status = '',
    this.error = '',
    this.logs = const [],
    this.lines = const [],
    this.writes = 0,
    this.ms = 0,
  });
  final String status, error;
  final List<String> logs, lines;
  final int writes, ms;
}


/// Plugin = a code space listed by kind (`mnnn plugins list`).
class PluginInfo {
  const PluginInfo({
    required this.name,
    required this.kind,
    this.lang = '',
    this.main = '',
    this.approved = false,
    this.enabled = true,
  });
  final String name, kind, lang, main;
  final bool approved, enabled;
}

class RulesetInfo {
  const RulesetInfo({
    required this.id,
    required this.name,
    this.description = '',
    this.ownerKind = 'standalone',
    this.enabled = true,
    this.rules = const [],
    this.schedules = const [],
  });
  final String id, name, description, ownerKind;
  final bool enabled;
  final List<String> rules, schedules;
}

class ScheduleInfo {
  const ScheduleInfo({
    required this.id,
    required this.name,
    this.targetKind = '',
    this.target = '',
    this.runEvery = '',
    this.cron = '',
    this.tz = '',
    this.startAt = 0,
    this.endsAt = 0,
    this.enabled = true,
    this.nextRunAt = 0,
    this.overlap = 'skip',
    this.catchUp = 'once',
    this.maxAttempts = 3,
    this.describe = const [],
  });
  final String id, name, targetKind, target, runEvery, cron, tz, overlap, catchUp;
  final int startAt, endsAt, nextRunAt, maxAttempts;
  final bool enabled;
  final List<String> describe;
}

class ScheduleRunInfo {
  const ScheduleRunInfo({
    required this.id,
    this.status = '',
    this.timestamp = 0,
    this.attempt = 1,
    this.startedAt = 0,
    this.finishedAt = 0,
    this.rulesetRan = 0,
    this.stored = 0,
    this.error = '',
  });
  final int id, timestamp, attempt, startedAt, finishedAt, rulesetRan, stored;
  final String status, error;
}

/// Outcome of create-then-share (`mnnn share --create` / `--create-to`).
class ShareCreated {
  const ShareCreated({
    this.createdItemKind = '',
    this.createdItemId = '',
    this.createdItemName = '',
    this.createdToKind = '',
    this.createdToId = '',
    this.createdToName = '',
    this.summary = '',
    this.itemId = '',
    this.itemName = '',
    this.target = '',
    this.to = '',
    this.made = '',
    this.newShare = false,
  });
  final String createdItemKind, createdItemId, createdItemName;
  final String createdToKind, createdToId, createdToName;
  final String summary, itemId, itemName, target, to, made;
  final bool newShare;
}

class LiveHopInfo {
  const LiveHopInfo({
    required this.name,
    this.status = 'waiting',
    this.rows = 0,
    this.ms = 0,
    this.columns = const [],
    this.error = '',
    this.state = '',
    this.rowsIn = 0,
  });
  final String name, status, error, state;
  final int rows, ms, rowsIn;
  final List<String> columns;
}

class LivePipeInfo {
  const LivePipeInfo({
    required this.token,
    this.origin = '',
    this.status = 'running',
    this.pipeline = '',
    this.hops = const [],
    this.saved = const [],
    this.updated = 0,
  });
  final String token, origin, status, pipeline;
  final List<LiveHopInfo> hops;
  final List<String> saved;
  final int updated;
}

class PipeRunResult {
  const PipeRunResult({
    this.hops = const [],
    this.saved = const [],
    this.lines = const [],
  });
  final List<String> hops, saved, lines;
}

class PipelineInfo {
  const PipelineInfo({
    required this.id,
    required this.shortId,
    required this.name,
    this.description = '',
    this.version = 1,
    this.draft = false,
    this.nodes = 0,
    this.sinks = '',
    this.problems = const [],
    this.nodeSpecs = const [],
    this.edgeSpecs = const [],
    this.graphLines = const [],
  });
  final String id, shortId, name, description, sinks;
  final int version, nodes;
  final bool draft;
  final List<String> problems, nodeSpecs, edgeSpecs, graphLines;
}

class PipelineRunResult {
  const PipelineRunResult({
    this.ok = true,
    this.lines = const [],
    this.landed = const [],
  });
  final bool ok;
  final List<String> lines, landed;
}

class PipelineMonitorNode {
  const PipelineMonitorNode({
    required this.id,
    this.kind = '',
    this.status = 'idle',
    this.rowsIn = 0,
    this.rowsOut = 0,
    this.ms = 0,
  });
  final String id, kind, status;
  final int rowsIn, rowsOut, ms;
}

class PipelineMonitor {
  const PipelineMonitor({
    required this.pipeline,
    this.state = 'not run yet',
    this.nodes = const [],
    this.landed = const [],
  });
  final String pipeline, state;
  final List<PipelineMonitorNode> nodes;
  final List<String> landed;
}

class McpServerInfo {
  const McpServerInfo({
    required this.id,
    required this.name,
    this.transport = '',
    this.command = '',
    this.args = const [],
    this.url = '',
    this.credentials = const [],
    this.createdAt = 0,
  });
  final String id, name, transport, command, url;
  final List<String> args, credentials;
  final int createdAt;
}

class McpClientInfo {
  const McpClientInfo({
    required this.id,
    required this.name,
    this.role = 'read',
    this.createdAt = 0,
    this.lastSeenAt = 0,
    this.revoked = false,
  });
  final String id, name, role;
  final int createdAt, lastSeenAt;
  final bool revoked;
}

class McpToolInfo {
  const McpToolInfo({
    required this.name,
    this.description = '',
    this.approved = false,
  });
  final String name, description;
  final bool approved;
}

class PeerMessageInfo {
  const PeerMessageInfo({
    required this.id,
    required this.peer,
    this.peerLabel = '',
    this.direction = 'in',
    this.about = '',
    this.text = '',
    this.createdAt = 0,
    this.delivered = false,
    this.read = false,
    this.error = '',
  });
  final String id, peer, peerLabel, direction, about, text, error;
  final int createdAt;
  final bool delivered, read;
}

class LiveSessionInfo {
  const LiveSessionInfo({
    required this.id,
    this.kind = '',
    this.title = '',
    this.role = '',
    this.hostPeer = '',
    this.status = '',
    this.createdAt = 0,
    this.updatedAt = 0,
  });
  final String id, kind, title, role, hostPeer, status;
  final int createdAt, updatedAt;
}

class RuleInfo {
  const RuleInfo({
    required this.id,
    required this.shortId,
    required this.name,
    this.description = '',
    this.enabled = true,
    this.home = '',
    this.brief = '',
    this.onText = '',
    this.whenText = '',
    this.thenText = '',
    this.related = false,
    this.runs = 0,
    this.lastStatus = '',
    this.lastRunAt = 0,
    this.nextRun = 0,
    this.createdAt = 0,
  });
  final String id,
      shortId,
      name,
      description,
      home,
      brief,
      onText,
      whenText,
      thenText,
      lastStatus;
  final bool enabled, related;
  final int runs, lastRunAt, nextRun, createdAt;
}

class RuleEventInfo {
  const RuleEventInfo({required this.kind, this.about = '', this.fields = ''});
  final String kind, about, fields;
}

class NotificationInfo {
  const NotificationInfo({
    required this.id,
    required this.shortId,
    required this.title,
    this.kind = '',
    this.level = 'info',
    this.body = '',
    this.link = '',
    this.linkLabel = '',
    this.actionId = 0,
    this.createdAt = 0,
    this.read = false,
    this.readAt = 0,
  });
  final String id, shortId, title, kind, level, body, link, linkLabel;
  final int actionId, createdAt, readAt;
  final bool read;
}

class NotificationKindInfo {
  const NotificationKindInfo({
    required this.kind,
    this.about = '',
    this.muted = false,
  });
  final String kind, about;
  final bool muted;
}

class ReviewInfo {
  const ReviewInfo({
    required this.id,
    required this.shortId,
    required this.title,
    this.kind = '',
    this.name = '',
    this.note = '',
    this.status = 'open',
    this.allow = '',
    this.items = 0,
    this.votesApprove = 0,
    this.votesChanges = 0,
    this.votesReject = 0,
    this.createdAt = 0,
    this.updatedAt = 0,
  });
  final String id, shortId, title, kind, name, note, status, allow;
  final int items,
      votesApprove,
      votesChanges,
      votesReject,
      createdAt,
      updatedAt;
}

class ReviewerInfo {
  const ReviewerInfo({
    required this.id,
    required this.shortId,
    required this.title,
    this.status = '',
    this.label = '',
  });
  final String id, shortId, title, status, label;
}

class ReviewItemInfo {
  const ReviewItemInfo({
    required this.id,
    required this.shortId,
    required this.line,
    this.kind = '',
    this.who = '',
    this.state = '',
    this.field = '',
    this.target = '',
    this.quote = '',
    this.body = '',
    this.value = '',
    this.createdAt = 0,
  });
  final String id,
      shortId,
      line,
      kind,
      who,
      state,
      field,
      target,
      quote,
      body,
      value;
  final int createdAt;
}

class ReviewDetail {
  const ReviewDetail({
    required this.review,
    this.mine = true,
    this.note = '',
    this.reviewers = const [],
    this.items = const [],
    this.youMay = '',
    this.peer = '',
  });
  final ReviewInfo review;
  final bool mine;
  final String note, youMay, peer;
  final List<ReviewerInfo> reviewers;
  final List<ReviewItemInfo> items;
}

class ReviewingInfo {
  const ReviewingInfo({
    required this.id,
    required this.shortId,
    required this.title,
    this.kind = '',
    this.name = '',
    this.status = '',
    this.allow = '',
    this.peer = '',
    this.items = 0,
    this.updatedAt = 0,
  });
  final String id, shortId, title, kind, name, status, allow, peer;
  final int items, updatedAt;
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

/// `mnnn net config` / TUI Net ▸ Settings… (relays stay on the Relays page).
class NetConfig {
  const NetConfig({
    this.label = '',
    this.port = 4741,
    this.listen = const [],
    this.mdns = true,
    this.relayServer = false,
    this.autostart = true,
    this.hostAutostart = false,
    this.accept = 'known',
    this.bootstrap = const [],
    this.externalAddresses = const [],
    this.collabTransport = 'stream',
    this.collabBind = '0.0.0.0',
    this.collabPort = 4742,
    this.collabAdvertise = const [],
  });
  final String label, accept, collabTransport, collabBind;
  final int port, collabPort;
  final List<String> listen, bootstrap, externalAddresses, collabAdvertise;
  final bool mdns, relayServer, autostart, hostAutostart;
}

class NetMeInfo {
  const NetMeInfo({
    required this.userId,
    required this.shortId,
    required this.name,
    this.nodes = 0,
  });
  final String userId, shortId, name;
  final int nodes;
}

class NetConnNode {
  const NetConnNode({
    required this.shortId,
    this.label = '',
    this.active = false,
    this.state = '',
  });
  final String shortId, label, state;
  final bool active;
}

class NetConnectionInfo {
  const NetConnectionInfo({
    required this.userId,
    required this.shortId,
    required this.name,
    this.trusted = false,
    this.nodes = 0,
    this.since = 0,
    this.nodeList = const [],
  });
  final String userId, shortId, name;
  final bool trusted;
  final int nodes, since;
  final List<NetConnNode> nodeList;
}

class NetConnections {
  const NetConnections({this.me, this.connections = const []});
  final NetMeInfo? me;
  final List<NetConnectionInfo> connections;
}

/// A key exchange (`mnnn net exchange list` / TUI Nodes…). Tokens stay in Rust;
/// [shortCode] and [fingerprint] are meant to be read aloud.
class NetExchangeInfo {
  const NetExchangeInfo({
    required this.id,
    required this.shortId,
    this.title = '',
    this.label = '',
    this.status = '',
    this.role = '',
    this.peerId = '',
    this.peerLabel = '',
    this.shortCode,
    this.fingerprint,
    this.expiresAt,
    this.verifiedAt,
    this.revokedAt,
    this.createdAt = 0,
    this.active = false,
    this.incoming = false,
  });
  final String id, shortId, title, label, status, role, peerId, peerLabel;
  final String? shortCode, fingerprint;
  final int? expiresAt, verifiedAt, revokedAt;
  final int createdAt;
  final bool active, incoming;
}

class NetRemoteItem {
  const NetRemoteItem({
    required this.id,
    required this.kind,
    required this.name,
    this.about = '',
  });
  final String id, kind, name, about;
}

class NetEntrustedInfo {
  const NetEntrustedInfo({
    required this.setId,
    required this.index,
    required this.threshold,
    required this.total,
    this.exchange = '',
    this.status = '',
  });
  final String setId, exchange, status;
  final int index, threshold, total;
}

class NetHeldInfo {
  const NetHeldInfo({
    required this.id,
    required this.shortId,
    required this.setId,
    required this.index,
    required this.threshold,
    required this.total,
    this.owner = '',
    this.exchange = '',
  });
  final String id, shortId, setId, owner, exchange;
  final int index, threshold, total;
}

class NetShards {
  const NetShards({this.entrusted = const [], this.held = const []});
  final List<NetEntrustedInfo> entrusted;
  final List<NetHeldInfo> held;
}

/// Peek of `mnnn net exchange restore` (no keys). Identity would be replaced if true.
class NetExchangeBackupPeek {
  const NetExchangeBackupPeek({
    required this.peerId,
    this.exchanges = 0,
    this.identityReplaced = false,
  });
  final String peerId;
  final int exchanges;
  final bool identityReplaced;
}

class NetSearchOrigin {
  const NetSearchOrigin({
    required this.origin,
    this.peer,
    this.exchange,
    this.error,
    this.truncated = false,
    this.lines = const [],
  });
  final String origin;
  final String? peer, exchange, error;
  final bool truncated;
  final List<String> lines;
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

/// A site template (`mnnn org template`). HTML/CSS are the user's markup.
class OrgTemplateInfo {
  const OrgTemplateInfo({
    required this.name,
    this.inUse = false,
    this.updatedAt = '',
    this.assetNames = const [],
    this.buildCmd = '',
    this.buildSrc = '',
    this.buildOut = '',
    this.buildApproved = false,
    this.html = '',
    this.css = '',
  });
  final String name;
  final bool inUse;
  final String updatedAt;
  final List<String> assetNames;
  final String buildCmd;
  final String buildSrc;
  final String buildOut;
  final bool buildApproved;
  final String html;
  final String css;
}

class OrgDomainInfo {
  const OrgDomainInfo({required this.org, required this.domain});
  final String org;
  final String domain;
}

/// Certificate metadata only. No PEM chain or private key.
class OrgCertInfo {
  const OrgCertInfo({
    required this.org,
    required this.slug,
    this.domains = const [],
    this.source = '',
    this.names = const [],
    this.notAfter = '',
    this.issuer = '',
    this.acmeEnabled = false,
    this.acmeEmail = '',
    this.retryAttempts = 0,
    this.retryNext = '',
    this.retryError = '',
  });
  final String org;
  final String slug;
  final List<String> domains;
  final String source;
  final List<String> names;
  final String notAfter;
  final String issuer;
  final bool acmeEnabled;
  final String acmeEmail;
  final int retryAttempts;
  final String retryNext;
  final String retryError;
}

class OrgNodeCertInfo {
  const OrgNodeCertInfo({
    this.source = '',
    this.names = const [],
    this.notAfter = '',
    this.issuer = '',
    this.acmeEnabled = false,
    this.acmeDomain = '',
    this.acmeEmail = '',
    this.acmeDirectory = '',
    this.acmeProfile = '',
    this.acmeChallenge = '',
    this.acmeHttpPort = 0,
    this.selfSignedDays = 0,
  });
  final String source;
  final List<String> names;
  final String notAfter;
  final String issuer;
  final bool acmeEnabled;
  final String acmeDomain;
  final String acmeEmail;
  final String acmeDirectory;
  final String acmeProfile;
  final String acmeChallenge;
  final int acmeHttpPort;
  final int selfSignedDays;
}

class OrgCertOverview {
  const OrgCertOverview({
    this.node = const OrgNodeCertInfo(),
    this.orgs = const [],
  });
  final OrgNodeCertInfo node;
  final List<OrgCertInfo> orgs;
}

class OrgBrowseItem {
  const OrgBrowseItem({
    required this.name,
    required this.slug,
    this.visibility = '',
    this.access = '',
    this.members = 0,
    this.description = '',
  });
  final String name;
  final String slug;
  final String visibility;
  final String access;
  final int members;
  final String description;
}

class OrgFetchResult {
  const OrgFetchResult({
    required this.name,
    this.format = '',
    this.size = 0,
    this.text,
    this.written = '',
  });
  final String name;
  final String format;
  final int size;
  final String? text;
  final String written;
}

class OrgSyncCodeItem {
  const OrgSyncCodeItem({required this.name, this.state = ''});
  final String name;
  final String state;
}

class OrgCodeServedInfo {
  const OrgCodeServedInfo({
    required this.name,
    this.lang = '',
    this.shortId = '',
  });
  final String name;
  final String lang;
  final String shortId;
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

  /// Named workspace layouts in `[tui.layouts]`, shared with the TUI.
  Future<List<String>> savedLayouts();
  Future<String> saveNamedLayout(String name, String json);
  Future<String> namedLayout(String name);
  Future<String> removeNamedLayout(String name);

  /// Live workspace tree (`[tui.layout]`), shared with the TUI.
  Future<String?> liveLayout();
  Future<String> saveLiveLayout(String json);

  /// History bar visibility (`[tui.history_open]`, TUI `Y`).
  Future<bool> historyOpen();
  Future<void> setHistoryOpen(bool open);

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

  /// `mnnn sources list`. The store is opened read-only.
  Future<List<SourceInfo>> listSources();

  /// `mnnn sources add`. [kind] defaults to `file` when empty.
  Future<SourceInfo> addSource(
    String name, {
    String kind = 'file',
    String? locator,
  });

  /// `mnnn sources rename`. Returns how many bar ids were recomputed.
  Future<int> renameSource(String from, String to);

  /// `mnnn sources remove`. Deletes the source, its bars and its stored items.
  Future<int> removeSource(String name);

  /// `mnnn sources data NAME`. Metadata only; bytes stay in Rust.
  Future<List<SourceDataItem>> listSourceData(String name);

  /// `mnnn sources cat NAME ITEM` written to [path]. The blob never enters Dart.
  Future<int> exportSourceData(String name, String item, String path);

  /// `mnnn symbols list` (the pairs).
  Future<List<SymbolInfo>> listSymbols();

  /// `alias A -> C` lines from `mnnn symbols list`.
  Future<List<AliasInfo>> listAliases();

  /// `mnnn symbols add`. [aliases] apply to the base asset.
  Future<SymbolInfo> addSymbol(
    String symbol, {
    List<String> aliases = const [],
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

  /// Every item in a project (`mnnn project show`).
  Future<List<ProjectItemInfo>> projectItems(String projectId);

  /// `mnnn project add --journal|--search|--report|--rule|--model|--code`.
  Future<String> addProjectItem(String project, String kind, String key);

  /// `mnnn project import` a `.mnnp` archive or JSON pack.
  Future<String> projectImport(String path, {String? name});

  /// `mnnn project export` as archive (`.mnnp`) or JSON. `noData` leaves bars out.
  Future<String> projectExport(
    String id,
    String path, {
    String format = 'archive',
    bool noData = false,
  });

  Future<List<JournalInfo>> journals();
  Future<JournalInfo> createJournal(
    String name, {
    String? project,
    String format = 'markdown',
  });
  Future<JournalInfo> updateJournal(String id, {String? name, String? format});
  Future<String> removeJournal(String id);
  Future<JournalInfo> forkJournal(String id, {String? project, String? name});
  Future<List<JournalEntryInfo>> journalEntries(String journal);
  Future<JournalEntryInfo> addJournalEntry(
    String journal, {
    String title = '',
    String body = '',
    String? format,
  });
  Future<JournalEntryInfo> updateJournalEntry(
    String entry, {
    String? title,
    String? body,
    String? format,
  });
  Future<String> removeJournalEntry(String entry);
  Future<List<JournalNoteInfo>> journalNotes(String journal);
  Future<JournalNoteInfo> addJournalNote(String journal, String body);

  Future<List<JournalAnnotationInfo>> entryAnnotations(String entry);
  Future<JournalAnnotationInfo> addEntryNote(
    String entry,
    String body, {
    String? words,
    int? nth,
    int? start,
    int? end,
  });
  Future<JournalAnnotationInfo> addEntryHighlight(
    String entry, {
    String? words,
    int? nth,
    int? start,
    int? end,
    String? color,
    String? comment,
  });
  Future<JournalAnnotationInfo> updateJournalAnnotation(String id, String body);
  Future<String> removeJournalAnnotation(String id);
  Future<List<JournalImageInfo>> entryImages(String entry);
  Future<JournalImageInfo> attachJournalImage(
    String entry,
    String path, {
    String? name,
  });
  Future<String> saveJournalImage(String id, String out);

  /// Downscaled PNG preview. Empty when the attachment is not a decodable
  /// image. Original bytes stay in Rust.
  Future<Uint8List> journalImageThumbnail(String id, {int? maxPx});
  Future<List<JournalEntryInfo>> copyJournalEntries(
    List<String> entries, {
    String? to,
    String? newName,
    String? project,
  });
  Future<JournalExportInfo> exportJournal(
    String journal, {
    String format = 'pdf',
    required String out,
    List<String> entries = const [],
  });
  Future<JournalInfo> importJournal(
    String path, {
    String? project,
    String? name,
  });
  Future<JournalExportInfo> printJournal(
    String journal, {
    List<String> entries = const [],
    String? printer,
    String? out,
    bool saveOnly = true,
  });

  Future<List<SearchInfo>> searches();
  Future<SearchInfo> saveSearch(
    String name, {
    String query = '',
    String target = 'series',
    List<String> sources = const [],
    List<String> symbols = const [],
    List<String> periods = const [],
    List<int> years = const [],
    int? from,
    int? to,
    String? price,
    double? min,
    double? max,
    String? sort,
    bool desc = false,
    String? model,
    List<String> conditions = const [],
  });
  Future<SearchInfo> editSearch(
    String id, {
    String? name,
    String query = '',
    String target = 'series',
    List<String> sources = const [],
    List<String> symbols = const [],
    List<String> periods = const [],
    List<int> years = const [],
    int? from,
    int? to,
    String? price,
    double? min,
    double? max,
    String? sort,
    bool desc = false,
    String? model,
    List<String> conditions = const [],
  });
  Future<SearchInfo> renameSearch(String id, String name);
  Future<SearchInfo> forkSearch(String id, {String? name});
  Future<String> removeSearch(String id);

  /// Journal entries / records matching a live search (TUI apply → found-views).
  Future<List<FoundEntryInfo>> searchFoundEntries(
    String query, {
    int? from,
    int? to,
  });
  Future<List<FoundRecordInfo>> searchFoundRecords(
    String query, {
    int? from,
    int? to,
  });

  /// Models and records (`mnnn model` / TUI Data ▸ Model designer…).
  Future<List<ModelInfo>> models();
  Future<ModelInfo> model(String key);
  Future<List<RecordInfo>> modelRecords(
    String model, {
    String? text,
    int? limit,
  });
  Future<RecordInfo> modelRecord(String model, String id);
  Future<List<String>> modelFieldTypes();
  Future<ModelInfo> modelCreate(
    String name, {
    String description = '',
    String fields = '',
    bool draft = true,
  });
  Future<ModelInfo> modelRename(
    String key, {
    String? name,
    String? description,
  });
  Future<ModelInfo> modelEdit(
    String key, {
    List<String> add = const [],
    List<String> change = const [],
    List<String> drop = const [],
    List<String> rename = const [],
    List<String> moves = const [],
  });
  Future<ModelInfo> modelSetField(String key, String fieldId, String spec);
  Future<ModelPlanInfo?> modelPlan(String key);
  Future<ModelPlanInfo> modelApply(String key, {bool destructive = false});
  Future<ModelInfo> modelDiscard(String key);
  Future<List<ModelVersionInfo>> modelHistory(String key);
  Future<ModelPlanInfo> modelRollbackPlan(String key, int version);
  Future<ModelPlanInfo> modelRollback(
    String key,
    int version, {
    bool destructive = false,
  });
  Future<String> modelRemove(String key, {bool destructive = false});
  Future<List<String>> modelRuleIds(String key);
  Future<ModelInfo> modelRestoreField(String key, String fieldId);
  Future<List<ModelTemplateInfo>> modelTemplates();
  Future<String> modelExportTemplate(
    String key,
    String path, {
    bool records = false,
  });
  Future<ModelImportPlanInfo> modelImportPreview(
    String source, {
    String? name,
    bool merge = false,
    bool draft = false,
    bool noRecords = false,
  });
  Future<String> modelImportTemplate(
    String source, {
    String? name,
    bool merge = false,
    bool draft = false,
    bool noRecords = false,
    bool destructive = false,
  });
  Future<RecordInfo> modelRecordCreate(String model, String values);
  Future<RecordInfo> modelRecordUpdate(String model, String id, String values);
  Future<String> modelRecordUpdateMany(
    String model,
    String text,
    String values,
  );
  Future<String> modelRecordDelete(String model, String id);
  Future<List<String>> modelRuleTry(String rule);

  /// Action history (`View ▸ History`, TUI `Y` / `u` / `U`).
  Future<List<HistoryRowInfo>> historyTree();
  Future<HistoryMoveInfo> historyBack();
  Future<HistoryMoveInfo> historyForward();
  Future<HistoryMoveInfo> historyGoto(int? id);

  /// File ▸ Authors… (TUI `A`).
  Future<List<AuthorInfo>> authors();
  Future<AuthorInfo> addAuthor(
    String name, {
    String? email,
    List<String> details = const [],
  });
  Future<AuthorInfo> updateAuthor(
    String id, {
    String? name,
    String? email,
    List<String> details = const [],
    bool replaceDetails = false,
  });
  Future<String> removeAuthor(String id);
  Future<String> setCurrentAuthor(String? id);

  Future<List<ReportInfo>> reports();
  Future<ReportInfo> createReport(
    String name, {
    String description = '',
    String? project,
  });
  Future<ReportInfo> updateReport(
    String id, {
    String? name,
    String? description,
  });
  Future<String> removeReport(String id);
  Future<List<ReportSectionInfo>> reportSections(String report);
  Future<ReportSectionInfo> addReportSection(
    String report, {
    String kind = 'text',
    String title = '',
    String body = '',
    String format = 'markdown',
    String? target,
    int? from,
    int? to,
    int? rows,
  });
  Future<ReportExportInfo> exportReport(
    String report, {
    String format = 'pdf',
    required String out,
  });
  Future<String> previewReport(String report, {int width = 100});
  Future<ReportSectionInfo> updateReportSection(
    String report,
    String section, {
    String? title,
    String? body,
    String? format,
  });
  Future<String> removeReportSection(String report, String section);
  Future<ReportSectionInfo> moveReportSection(
    String report,
    String section,
    int to,
  );

  /// `mnnn code list` / TUI Code spaces….
  Future<List<CodeSpaceInfo>> codeSpaces();
  Future<CodeSpaceInfo> codeCreate(
    String name, {
    String lang = 'math',
    String description = '',
    String? source,
    String? from,
    String? input,
  });
  Future<CodeSpaceInfo> codeEdit(
    String space, {
    String? name,
    String? description,
    String? lang,
    String? source,
    String? input,
  });
  Future<String> codeRemove(String space);
  Future<String> codeApprove(String space);
  Future<List<String>> codeCheck(String space);
  Future<CodeRunResult> codeBuild(String space);
  Future<CodeRunResult> codeRun(String space);

  /// Latest `pipe.frame` for a data tile (`code:SPACE` or `view:PIPELINE/NODE`).
  Future<PipeFrameInfo?> latestPipeFrame(String key);
  Future<List<CodeParserInfo>> codeParsers();
  Future<String> codeParsersInstall({String? name});

  /// TUI Pipes… (live history) and `mnnn pipe SOURCE STEP…`.
  Future<List<LivePipeInfo>> pipesLive();
  Future<PipeRunResult> pipeRun(
    String source, {
    List<String> steps = const [],
    List<String> to = const [],
    String? stdin,
    String? stdinFile,
    String? format,
    int chunk = 0,
    bool keepHeader = false,
  });

  /// `mnnn pipeline` / TUI Pipelines… (`E`).
  Future<List<PipelineInfo>> pipelines();
  Future<PipelineInfo> pipelineCreate(
    String name, {
    String description = '',
    List<String> nodes = const [],
    List<String> edges = const [],
  });
  Future<PipelineInfo> pipelineEdit(
    String pipeline, {
    String? name,
    String? description,
    List<String> nodes = const [],
    List<String> edges = const [],
    List<String> removeNodes = const [],
    List<String> removeEdges = const [],
  });
  Future<String> pipelineRemove(String pipeline);
  Future<String> pipelineApprove(String pipeline);
  Future<PipelineRunResult> pipelineRun(
    String pipeline, {
    String? stdin,
    String? stdinFile,
    int chunk = 0,
    bool keepHeader = false,
  });
  Future<PipelineInfo> pipelineTee(String pipeline, String node, String sink);
  Future<PipelineInfo> pipelineEndInModel(
    String pipeline,
    String node,
    String model,
  );
  Future<PipelineMonitor> pipelineMonitor(String pipeline);

  /// TUI Net ▸ MCP… (`mnnn mcp`). Tokens and credential values stay in Rust.
  Future<List<McpServerInfo>> mcpServers();
  Future<List<McpClientInfo>> mcpClients();
  Future<List<McpToolInfo>> mcpServerTools(String server);
  Future<String> mcpApproveTool(String server, String tool, {bool on = true});
  Future<String> mcpRemoveServer(String server);
  Future<String> mcpClientRole(String client, String role);
  Future<String> mcpClientRevoke(String client, {bool revoke = true});
  Future<String> mcpRemoveClient(String client);

  /// TUI `W` messages and live sessions (listing what the store holds).
  Future<List<PeerMessageInfo>> peerMessages({String? peer});
  Future<String> peerMessagesMarkRead({String? peer});
  Future<List<LiveSessionInfo>> liveSessions();
  Future<String> pipelineExport(
    String pipeline,
    String path, {
    bool templates = true,
  });
  Future<List<String>> pipelineImportPlan(
    String path, {
    String? name,
    bool merge = false,
  });
  Future<String> pipelineImport(
    String path, {
    String? name,
    bool merge = false,
  });

  /// `mnnn rule` / TUI Rules… (`R`).
  Future<List<RuleInfo>> rules();
  Future<RuleInfo> ruleCreate(
    String name, {
    String description = '',
    String on = '',
    String when = '',
    String then = '',
    String home = '',
    bool related = false,
  });
  Future<RuleInfo> ruleEdit(
    String rule, {
    String? name,
    String? description,
    String? on,
    String? when,
    String? then,
    String? home,
    bool? related,
    bool noSchedule = false,
  });
  Future<String> ruleRemove(String rule);
  Future<String> ruleEnable(String rule);
  Future<String> ruleDisable(String rule);
  Future<List<String>> ruleShow(String rule);
  Future<List<String>> ruleTest({String? rule, int? action});
  Future<List<String>> ruleDryRun({String? rule, int? action});
  Future<String> ruleTick();
  Future<List<RuleEventInfo>> ruleEvents({String? kind});

  /// `mnnn notify` / TUI Notifications… (`!`).
  Future<List<NotificationInfo>> notifications({
    bool unread = false,
    int limit = 50,
  });
  Future<int> notificationsUnreadCount();
  Future<NotificationInfo> notificationShow(String id);
  Future<String> notificationRead(String id);
  Future<String> notificationUnread(String id);
  Future<String> notificationReadAll();
  Future<NotificationInfo> notificationSend(
    String title, {
    String body = '',
    String? link,
    String level = 'info',
  });
  Future<List<NotificationKindInfo>> notificationKinds();
  Future<String> notificationMute(String kind);
  Future<String> notificationUnmute(String kind);

  /// `mnnn review` / TUI Reviews… (`V`). Exchange tokens and the node key stay in Rust.
  Future<List<ReviewInfo>> reviews();
  Future<List<ReviewingInfo>> reviewsJoined();
  Future<ReviewInfo> reviewRequest(
    String kind,
    String target, {
    String title = '',
    String note = '',
    List<String> allow = const [],
  });
  Future<ReviewDetail> reviewShow(String review);
  Future<List<String>> reviewView(String review);
  Future<String> reviewInvite(String review, {String label = '', String? peer});
  Future<String> reviewJoin(String token, {String label = ''});
  Future<String> reviewSync(String review);
  Future<String> reviewComment(
    String review,
    String text, {
    String? field,
    String? at,
    int nth = 1,
    String? entry,
    String? reply,
  });
  Future<String> reviewSuggest(
    String review,
    String to, {
    String why = '',
    String? field,
    String? at,
    int nth = 1,
    String? entry,
  });
  Future<String> reviewHighlight(
    String review, {
    String? field,
    String? at,
    int nth = 1,
    String? entry,
    String? color,
    String note = '',
  });
  Future<String> reviewVote(
    String review,
    String vote, {
    String? on,
    String note = '',
  });
  Future<String> reviewWithdraw(String review, String item);
  Future<String> reviewAccept(
    String review, {
    List<String> items = const [],
    bool all = false,
  });
  Future<String> reviewReject(
    String review, {
    List<String> items = const [],
    bool all = false,
  });
  Future<String> reviewMerge(String review, {bool force = false});
  Future<String> reviewClose(String review);
  Future<String> reviewReopen(String review);
  Future<String> reviewDrop(String review, String reviewer);
  Future<String> reviewLeave(String review);
  Future<String> reviewRemove(String review);
  Future<List<String>> reviewRevisions(String kind, String target);

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

  /// `mnnn net config` / TUI Settings. Does not create preferences.toml on read.
  Future<NetConfig> netConfig();
  Future<String> netConfigSet(NetConfig cfg);

  /// `mnnn connection list` / TUI Connections…. No user keys leave Rust.
  Future<NetConnections> netConnections();
  Future<String> netConnectionTrust(String user, bool trusted);
  Future<String> netConnectionRemove(String user);

  /// `mnnn net exchange list` / TUI Nodes…. Tokens stay in Rust.
  Future<List<NetExchangeInfo>> netExchanges();
  Future<String> netExchangeLabel(String exchange, String label);
  Future<String> netExchangeVerify(String exchange, String code);
  Future<String> netExchangeRevoke(String exchange);
  Future<List<NetRemoteItem>> netRemoteList(String exchange);

  /// Fetch a peer's shared item. Bytes stay in Rust ([import] or write [path]).
  Future<String> netFetch(
    String exchange,
    String item, {
    String? format,
    String? path,
    bool import = false,
  });

  /// `mnnn net shard list` / TUI Dark shards. Sealed blobs stay in Rust.
  Future<NetShards> netShards();
  Future<String> netShardEntrust({
    required List<String> to,
    required String keyFile,
    required String shardPassword,
    String? filePassword,
    int threshold = 0,
  });
  Future<String> netShardCheck(String shardPassword);
  Future<String> netShardDrop(String id);

  /// `mnnn net search` / TUI Network search… (a saved live search here and on peers).
  Future<List<NetSearchOrigin>> netSearch(
    String saved, {
    List<String> only = const [],
    bool remoteOnly = false,
    int limit = 200,
  });

  /// `mnnn net broadcast`. [to] is `nodes`, `connections`, `all` or `org:SLUG`.
  Future<String> netBroadcast(String topic, String text, {String to = 'nodes'});

  /// `mnnn net exchange export`. Writes an age-encrypted backup; bytes stay in Rust.
  Future<String> netExchangeExport({
    required String out,
    required String password,
  });

  /// Peek a backup without restoring. Decrypts in Rust; no keys come back.
  Future<NetExchangeBackupPeek> netExchangeRestorePeek({
    required String file,
    required String password,
  });

  /// `mnnn net exchange restore`. Pass [confirmReplace] when peek said the identity would change.
  Future<String> netExchangeRestore({
    required String file,
    required String password,
    bool confirmReplace = false,
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

  Future<List<OrgTemplateInfo>> orgTemplates(String org);
  Future<OrgTemplateInfo> orgTemplateShow(String org, String template);
  Future<OrgTemplateInfo> orgTemplateSave(
    String org,
    String name, {
    String html = '',
    String css = '',
    bool useIt = false,
  });
  Future<String> orgTemplateRemove(String org, String template);
  Future<String> orgTemplateAddAsset(
    String org,
    String template,
    String path, {
    String? name,
  });
  Future<String> orgTemplateRemoveAsset(
    String org,
    String template,
    String name,
  );
  Future<String> orgTemplateBuild(
    String org,
    String template, {
    String? cmd,
    String? src,
    String? out,
    bool clear = false,
    bool noRun = false,
    bool approve = false,
  });
  Future<String> orgTemplatePreview(String org, String template);

  Future<List<OrgDomainInfo>> orgDomains({String? org});
  Future<String> orgDomainAdd(String org, String domain);
  Future<String> orgDomainRemove(String org, String domain);

  Future<OrgCertOverview> orgCertStatus({String? org});
  Future<String> orgCertSet(String org, String cert, String key);
  Future<String> orgCertClear(String org);
  Future<String> orgCertAcme({
    String? org,
    String? challenge,
    String? email,
    String? domain,
    String? directory,
    String? profile,
    int? httpPort,
    bool off = false,
  });
  Future<String> orgCertRenew({String? org});

  Future<List<OrgBrowseItem>> orgBrowse(String target, {String? org});
  Future<OrgFetchResult> orgFetch(
    String org,
    String resource, {
    String? format,
    String? out,
  });
  Future<String> orgGive(String org, String kind, String name);
  Future<String> orgMirror(String org);
  Future<List<OrgSyncCodeItem>> orgSyncCode(String org);
  Future<String> orgSignIn(String org, String site);

  Future<String> orgEndpointAdd(
    String org,
    String name,
    String kind,
    String target, {
    String? perm,
    bool anyone = false,
  });
  Future<String> orgEndpointRemove(String org, String name);
  Future<String> orgCall(String org, String endpoint, {String input = '{}'});

  Future<String> orgProjectAdd(String org, String project);
  Future<String> orgProjectRemove(String org, String project);
  Future<String> orgProjectMember(
    String org,
    String project,
    String member, {
    String? role,
  });

  Future<List<OrgCodeServedInfo>> orgCodeList(String org);
  Future<String> orgCodeServe(String org, String space);
  Future<String> orgCodeStop(String org, String space);


  /// Plugins by kind (`mnnn plugins list [--kind]`).
  Future<List<PluginInfo>> pluginsList({String? kind});
  Future<String> pluginSetEnabled(String name, bool enabled);

  /// Code space export/import (`mnnn spaces export|import`).
  Future<String> spaceExport(String space, String path);
  Future<List<String>> spaceImportPlan(String path, {String? name});
  Future<String> spaceImport(String path, {String? name});

  /// Rule sets and schedules (`mnnn ruleset` / `mnnn schedule`).
  Future<List<RulesetInfo>> rulesetsList();
  Future<RulesetInfo> rulesetCreate(String name, {String description = '', List<String> rules = const []});
  Future<RulesetInfo> rulesetEdit(String ruleset, {String? name, String? description, bool? enable});
  Future<String> rulesetRemove(String ruleset, {bool force = false});
  Future<RulesetInfo> rulesetAddRule(String ruleset, String rule, {int? position});
  Future<RulesetInfo> rulesetDropRule(String ruleset, String rule);

  Future<List<ScheduleInfo>> schedulesList();
  Future<List<ScheduleRunInfo>> scheduleRuns(String key);
  Future<ScheduleRunInfo> scheduleRunNow(String key);
  Future<ScheduleInfo> scheduleSetEnabled(String key, bool enabled);
  Future<String> schedulerTick();
  Future<String> schedulePastStart({
    String? runEvery,
    String? cron,
    String? startAt,
    String? endsAt,
    String? for_,
    String? tz,
  });
  Future<ScheduleInfo> scheduleAdd({
    required String name,
    String? runEvery,
    String? ruleset,
    String? plugin,
    String? source,
    String? pipeline,
    String? connectorRuleset,
    List<String> options = const [],
    String? cron,
    String? startAt,
    String? endsAt,
    String? for_,
    String? tz,
    bool pastNext = false,
    String? overlap,
    String? catchUp,
    int? maxAttempts,
    bool disabled = false,
  });
  Future<ScheduleInfo> scheduleEdit(
    String key, {
    String? name,
    String? runEvery,
    String? cron,
    String? startAt,
    String? endsAt,
    String? for_,
    String? tz,
    bool pastNext = false,
    String? overlap,
    String? catchUp,
    int? maxAttempts,
    bool clearEnds = false,
  });
  Future<String> scheduleRemove(String key);

  /// Create-then-share (`mnnn share --create` / `--create-to`).
  Future<ShareCreated> shareCreateItem(
    String kind,
    String name, {
    String description = '',
    String? lang,
    String? codeFrom,
    String? spaceKind,
    String? main,
    String? format,
    String? query,
  });
  Future<ShareCreated> shareCreateThen({
    required String kind,
    required String item,
    required String target,
    List<String> to = const [],
    String note = '',
    bool createItem = false,
    String? createTo,
    String description = '',
    String? lang,
    String? codeFrom,
    String? spaceKind,
    String? main,
    String? format,
    String? query,
    String? peer,
    int? hours,
  });
  Future<ShareCreated> shareCreateReviewSubject(
    String kind,
    String name, {
    String description = '',
    String? journal,
    String? body,
    String? model,
    String? format,
    String? query,
  });
  Future<ShareCreated> shareCreateProjectItem(String kind, String name, {String description = ''});

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
