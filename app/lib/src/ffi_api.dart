/// [MuninnApi] over the generated flutter_rust_bridge bindings (lib/src/rust/).
library;

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';

import 'api.dart';
import 'rust/api/about.dart' as r_about;
import 'rust/api/data.dart' as r_data;
import 'rust/api/info.dart' as r_info;
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
}
