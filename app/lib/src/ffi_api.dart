/// [MuninnApi] over the generated flutter_rust_bridge bindings (lib/src/rust/).
library;

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';

import 'api.dart';
import 'rust/api/about.dart' as r_about;
import 'rust/api/data.dart' as r_data;
import 'rust/api/info.dart' as r_info;
import 'rust/api/io.dart' as r_io;
import 'rust/api/prefs.dart' as r_prefs;
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
    final t = await r_themes.loadTheme(home: home, name: name, variant: variant);
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
    return SavedState(theme: s.theme, mode: s.mode, range: TimeRange(from: s.from?.toInt(), to: s.to?.toInt()), dataset: s.dataset);
  }

  @override
  Future<void> saveTheme(String name) => r_prefs.saveTheme(home: home, name: name);
  @override
  Future<void> saveThemeMode(String mode) => r_prefs.saveThemeMode(home: home, mode: mode);
  @override
  Future<void> saveTimeRange(TimeRange r) => r_prefs.saveTimeRange(
      home: home, from: r.from == null ? null : PlatformInt64Util.from(r.from!), to: r.to == null ? null : PlatformInt64Util.from(r.to!));
  @override
  Future<void> saveDataset(String? key) => r_prefs.saveDataset(home: home, key: key);

  @override
  Future<DataDir> dataDir() async {
    final d = await r_info.dataDirInfo(home: home);
    return DataDir(home: d.home, database: d.database, databaseExists: d.databaseExists, keysInitialized: d.keysInitialized);
  }

  @override
  Future<void> unlock(String password) => r_data.unlock(home: home, password: password);

  @override
  void lock() => r_data.lock();

  @override
  Future<Overview> overview() async {
    final o = await r_data.overview();
    return Overview(
      projects: [for (final p in o.projects) Project(id: p.id, shortId: p.shortId, name: p.name, description: p.description)],
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
            source: s.source,
            base: s.base,
            quote: s.quote,
            durationS: s.durationS.toInt(),
            from: s.from?.toInt(),
            to: s.to?.toInt(),
          ),
      ];

  @override
  Future<List<BarPoint>> queryBars(Dataset d, TimeRange range, {int maxPoints = 600}) async => [
        for (final b in await r_data.queryBars(
          source: d.source,
          base: d.base,
          quote: d.quote,
          durationS: PlatformInt64Util.from(d.durationS),
          from: range.from == null ? null : PlatformInt64Util.from(range.from!),
          to: range.to == null ? null : PlatformInt64Util.from(range.to!),
          maxPoints: maxPoints,
        ))
          BarPoint(ts: b.ts.toInt(), open: b.open, high: b.high, low: b.low, close: b.close, volume: b.volume),
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
}
