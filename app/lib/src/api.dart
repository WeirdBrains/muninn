/// What the UI needs from Muninn. [FfiMuninnApi] calls Rust (muninn-ffi); tests use a fake.
library;

class MuninnTheme {
  const MuninnTheme({required this.name, required this.displayName, required this.variant, required this.roles});
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
  final String name, version, tagline, motto, publisher, homepage, repository, license, copyright;
  final List<String> credits;
}

class DataDir {
  const DataDir({required this.home, required this.database, required this.databaseExists, required this.keysInitialized});
  final String home, database;
  final bool databaseExists, keysInitialized;
}

class Project {
  const Project({this.id = '', required this.shortId, required this.name, required this.description});
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
  bool overlaps(TimeRange r) => (r.from == null || last >= r.from!) && (r.to == null || first <= r.to!);
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
  bool operator ==(Object other) => other is TimeRange && other.from == from && other.to == to;
  @override
  int get hashCode => Object.hash(from, to);
}

class BarPoint {
  const BarPoint({required this.ts, required this.open, required this.high, required this.low, required this.close, required this.volume});
  final int ts;
  final double open, high, low, close, volume;
}

/// A series saved in a project, with the project's range.
class ProjectSeries {
  const ProjectSeries({this.itemId = 0, required this.source, required this.base, required this.quote, required this.durationS, this.from, this.to});
  final int itemId, durationS;
  final String source, base, quote;
  final int? from, to;
  String get key => '$source:$base/$quote:$durationS';
}

class Overview {
  const Overview({required this.projects, required this.datasets});
  final List<Project> projects;
  final List<Dataset> datasets;
}

/// What the app restores on launch from preferences.toml (shared with the TUI).
class SavedState {
  const SavedState({this.theme = 'tokyo-night', this.mode = 'system', this.range = TimeRange.all, this.dataset});
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
  const ExportResult({required this.path, required this.format, required this.rows});
  final String path, format;
  final int rows;
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
  Future<List<BarPoint>> queryBars(Dataset d, TimeRange range, {int maxPoints = 600});
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
}
