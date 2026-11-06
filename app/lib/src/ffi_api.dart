/// [MuninnApi] over the generated flutter_rust_bridge bindings (lib/src/rust/).
library;

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';

import 'api.dart';
import 'rust/api/about.dart' as r_about;
import 'rust/api/code.dart' as r_code;
import 'rust/api/data.dart' as r_data;
import 'rust/api/help.dart' as r_help;
import 'rust/api/info.dart' as r_info;
import 'rust/api/keys.dart' as r_keys;
import 'rust/api/io.dart' as r_io;
import 'rust/api/journals.dart' as r_journals;
import 'rust/api/net.dart' as r_net;
import 'rust/api/orgs.dart' as r_orgs;
import 'rust/api/prefs.dart' as r_prefs;
import 'rust/api/projects.dart' as r_projects;
import 'rust/api/reports.dart' as r_reports;
import 'rust/api/notify.dart' as r_notify;
import 'rust/api/reviews.dart' as r_reviews;
import 'rust/api/rules.dart' as r_rules;
import 'rust/api/searches.dart' as r_searches;
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
  Future<List<SourceInfo>> listSources() async => [
    for (final s in await r_data.listSources())
      SourceInfo(
        id: s.id,
        name: s.name,
        kind: s.kind,
        locator: s.locator,
        createdAt: s.createdAt.toInt(),
        items: s.items.toInt(),
        bytes: s.bytes.toInt(),
      ),
  ];

  @override
  Future<SourceInfo> addSource(
    String name, {
    String kind = 'file',
    String? locator,
  }) async {
    final s = await r_data.addSource(name: name, kind: kind, locator: locator);
    return SourceInfo(
      id: s.id,
      name: s.name,
      kind: s.kind,
      locator: s.locator,
      createdAt: s.createdAt.toInt(),
      items: s.items.toInt(),
      bytes: s.bytes.toInt(),
    );
  }

  @override
  Future<int> renameSource(String from, String to) async =>
      (await r_data.renameSource(from: from, to: to)).toInt();

  @override
  Future<int> removeSource(String name) async =>
      (await r_data.removeSource(name: name)).toInt();

  @override
  Future<List<SourceDataItem>> listSourceData(String name) async => [
    for (final i in await r_data.listSourceData(name: name))
      SourceDataItem(
        id: i.id,
        name: i.name,
        format: i.format,
        locator: i.locator,
        size: i.size.toInt(),
        contentHash: i.contentHash,
        receivedAt: i.receivedAt.toInt(),
      ),
  ];

  @override
  Future<int> exportSourceData(String name, String item, String path) async =>
      (await r_data.exportSourceData(
        name: name,
        item: item,
        path: path,
      )).toInt();

  @override
  Future<List<SymbolInfo>> listSymbols() async => [
    for (final s in await r_data.listSymbols())
      SymbolInfo(symbol: s.symbol, base: s.base, quote: s.quote),
  ];

  @override
  Future<List<AliasInfo>> listAliases() async => [
    for (final a in await r_data.listAliases())
      AliasInfo(alias: a.alias, canonical: a.canonical),
  ];

  @override
  Future<SymbolInfo> addSymbol(
    String symbol, {
    List<String> aliases = const [],
  }) async {
    final s = await r_data.addSymbol(symbol: symbol, aliases: aliases);
    return SymbolInfo(symbol: s.symbol, base: s.base, quote: s.quote);
  }

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
  Future<List<ProjectItemInfo>> projectItems(String projectId) async => [
    for (final i in await r_projects.projectItems(projectId: projectId))
      ProjectItemInfo(
        itemId: i.itemId.toInt(),
        kind: i.kind,
        name: i.name,
        detail: i.detail,
        refId: i.refId,
        source: i.source,
        base: i.base,
        quote: i.quote,
        durationS: i.durationS?.toInt(),
        from: i.from?.toInt(),
        to: i.to?.toInt(),
      ),
  ];

  @override
  Future<String> addProjectItem(String project, String kind, String key) =>
      r_projects.addProjectItem(project: project, kind: kind, key: key);

  @override
  Future<String> projectImport(String path, {String? name}) =>
      r_projects.projectImport(path: path, name: name);

  @override
  Future<String> projectExport(
    String id,
    String path, {
    String format = 'archive',
    bool noData = false,
  }) => r_projects.projectExport(
    id: id,
    path: path,
    format: format,
    noData: noData,
  );

  @override
  Future<List<JournalInfo>> journals() async => [
    for (final j in await r_journals.journals())
      JournalInfo(
        id: j.id,
        shortId: j.shortId,
        name: j.name,
        projectId: j.projectId,
        projectName: j.projectName,
        format: j.format,
        entries: j.entries.toInt(),
        forkedFrom: j.forkedFrom,
      ),
  ];

  @override
  Future<JournalInfo> createJournal(
    String name, {
    String? project,
    String format = 'markdown',
  }) async => _journal(
    await r_journals.createJournal(
      name: name,
      project: project,
      format: format,
    ),
  );

  @override
  Future<JournalInfo> updateJournal(
    String id, {
    String? name,
    String? format,
  }) async => _journal(
    await r_journals.updateJournal(id: id, name: name, format: format),
  );

  @override
  Future<String> removeJournal(String id) => r_journals.removeJournal(id: id);

  @override
  Future<JournalInfo> forkJournal(
    String id, {
    String? project,
    String? name,
  }) async => _journal(
    await r_journals.forkJournal(id: id, project: project, name: name),
  );

  @override
  Future<List<JournalEntryInfo>> journalEntries(String journal) async => [
    for (final e in await r_journals.journalEntries(journal: journal))
      _entry(e),
  ];

  @override
  Future<JournalEntryInfo> addJournalEntry(
    String journal, {
    String title = '',
    String body = '',
    String? format,
  }) async => _entry(
    await r_journals.addJournalEntry(
      journal: journal,
      title: title,
      body: body,
      format: format,
    ),
  );

  @override
  Future<JournalEntryInfo> updateJournalEntry(
    String entry, {
    String? title,
    String? body,
    String? format,
  }) async => _entry(
    await r_journals.updateJournalEntry(
      entry: entry,
      title: title,
      body: body,
      format: format,
    ),
  );

  @override
  Future<String> removeJournalEntry(String entry) =>
      r_journals.removeJournalEntry(entry: entry);

  @override
  Future<List<JournalNoteInfo>> journalNotes(String journal) async => [
    for (final n in await r_journals.journalNotes(journal: journal))
      JournalNoteInfo(id: n.id, shortId: n.shortId, body: n.body),
  ];

  @override
  Future<JournalNoteInfo> addJournalNote(String journal, String body) async {
    final n = await r_journals.addJournalNote(journal: journal, body: body);
    return JournalNoteInfo(id: n.id, shortId: n.shortId, body: n.body);
  }

  @override
  Future<List<JournalAnnotationInfo>> entryAnnotations(String entry) async => [
    for (final a in await r_journals.entryAnnotations(entry: entry)) _ann(a),
  ];

  @override
  Future<JournalAnnotationInfo> addEntryNote(
    String entry,
    String body, {
    String? words,
    int? nth,
    int? start,
    int? end,
  }) async => _ann(
    await r_journals.addEntryNote(
      entry: entry,
      body: body,
      words: words,
      nth: nth == null ? null : PlatformInt64Util.from(nth),
      start: start == null ? null : PlatformInt64Util.from(start),
      end: end == null ? null : PlatformInt64Util.from(end),
    ),
  );

  @override
  Future<JournalAnnotationInfo> addEntryHighlight(
    String entry, {
    String? words,
    int? nth,
    int? start,
    int? end,
    String? color,
    String? comment,
  }) async => _ann(
    await r_journals.addEntryHighlight(
      entry: entry,
      words: words,
      nth: nth == null ? null : PlatformInt64Util.from(nth),
      start: start == null ? null : PlatformInt64Util.from(start),
      end: end == null ? null : PlatformInt64Util.from(end),
      color: color,
      comment: comment,
    ),
  );

  @override
  Future<JournalAnnotationInfo> updateJournalAnnotation(
    String id,
    String body,
  ) async => _ann(await r_journals.updateJournalAnnotation(id: id, body: body));

  @override
  Future<String> removeJournalAnnotation(String id) =>
      r_journals.removeJournalAnnotation(id: id);

  @override
  Future<List<JournalImageInfo>> entryImages(String entry) async => [
    for (final i in await r_journals.entryImages(entry: entry)) _image(i),
  ];

  @override
  Future<JournalImageInfo> attachJournalImage(
    String entry,
    String path, {
    String? name,
  }) async => _image(
    await r_journals.attachJournalImage(entry: entry, path: path, name: name),
  );

  @override
  Future<String> saveJournalImage(String id, String out) =>
      r_journals.saveJournalImage(id: id, out: out);

  @override
  Future<List<JournalEntryInfo>> copyJournalEntries(
    List<String> entries, {
    String? to,
    String? newName,
    String? project,
  }) async => [
    for (final e in await r_journals.copyJournalEntries(
      entries: entries,
      to: to,
      newName: newName,
      project: project,
    ))
      _entry(e),
  ];

  @override
  Future<JournalExportInfo> exportJournal(
    String journal, {
    String format = 'pdf',
    required String out,
    List<String> entries = const [],
  }) async => _jexport(
    await r_journals.exportJournal(
      journal: journal,
      format: format,
      out: out,
      entries: entries,
    ),
  );

  @override
  Future<JournalInfo> importJournal(
    String path, {
    String? project,
    String? name,
  }) async => _journal(
    await r_journals.importJournal(path: path, project: project, name: name),
  );

  @override
  Future<JournalExportInfo> printJournal(
    String journal, {
    List<String> entries = const [],
    String? printer,
    String? out,
    bool saveOnly = true,
  }) async => _jexport(
    await r_journals.printJournal(
      journal: journal,
      entries: entries,
      printer: printer,
      out: out,
      saveOnly: saveOnly,
    ),
  );

  @override
  Future<List<SearchInfo>> searches() async => [
    for (final s in await r_searches.searches()) _search(s),
  ];

  Int64List _i64s(List<int> xs) => Int64List.fromList(xs);

  @override
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
  }) async => _search(
    await r_searches.saveSearchSpec(
      name: name,
      query: query,
      target: target,
      sources: sources,
      symbols: symbols,
      periods: periods,
      years: _i64s(years),
      from: from == null ? null : PlatformInt64Util.from(from),
      to: to == null ? null : PlatformInt64Util.from(to),
      price: price,
      min: min,
      max: max,
      sort: sort,
      desc: desc,
      model: model,
      conditions: conditions,
    ),
  );

  @override
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
  }) async => _search(
    await r_searches.editSearch(
      id: id,
      name: name,
      query: query,
      target: target,
      sources: sources,
      symbols: symbols,
      periods: periods,
      years: _i64s(years),
      from: from == null ? null : PlatformInt64Util.from(from),
      to: to == null ? null : PlatformInt64Util.from(to),
      price: price,
      min: min,
      max: max,
      sort: sort,
      desc: desc,
      model: model,
      conditions: conditions,
    ),
  );

  @override
  Future<SearchInfo> renameSearch(String id, String name) async =>
      _search(await r_searches.renameSearch(id: id, name: name));

  @override
  Future<SearchInfo> forkSearch(String id, {String? name}) async =>
      _search(await r_searches.forkSearch(id: id, name: name));

  @override
  Future<String> removeSearch(String id) => r_searches.removeSearch(id: id);

  @override
  Future<List<ReportInfo>> reports() async => [
    for (final r in await r_reports.reports()) _report(r),
  ];

  @override
  Future<ReportInfo> createReport(
    String name, {
    String description = '',
    String? project,
  }) async => _report(
    await r_reports.createReport(
      name: name,
      description: description,
      project: project,
    ),
  );

  @override
  Future<ReportInfo> updateReport(
    String id, {
    String? name,
    String? description,
  }) async => _report(
    await r_reports.updateReport(id: id, name: name, description: description),
  );

  @override
  Future<String> removeReport(String id) => r_reports.removeReport(id: id);

  @override
  Future<List<ReportSectionInfo>> reportSections(String report) async => [
    for (final s in await r_reports.reportSections(report: report)) _section(s),
  ];

  @override
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
  }) async => _section(
    await r_reports.addReportSection(
      report: report,
      kind: kind,
      title: title,
      body: body,
      format: format,
      target: target,
      from: from == null ? null : PlatformInt64Util.from(from),
      to: to == null ? null : PlatformInt64Util.from(to),
      rows: rows == null ? null : PlatformInt64Util.from(rows),
    ),
  );

  @override
  Future<ReportExportInfo> exportReport(
    String report, {
    String format = 'pdf',
    required String out,
  }) async {
    final r = await r_reports.exportReport(
      report: report,
      format: format,
      out: out,
    );
    return ReportExportInfo(
      path: r.path,
      files: r.files.toInt(),
      message: r.message,
    );
  }

  @override
  Future<String> previewReport(String report, {int width = 100}) => r_reports
      .previewReport(report: report, width: PlatformInt64Util.from(width));

  @override
  Future<ReportSectionInfo> updateReportSection(
    String report,
    String section, {
    String? title,
    String? body,
    String? format,
  }) async => _section(
    await r_reports.updateReportSection(
      report: report,
      section: section,
      title: title,
      body: body,
      format: format,
    ),
  );

  @override
  Future<String> removeReportSection(String report, String section) =>
      r_reports.removeReportSection(report: report, section: section);

  @override
  Future<ReportSectionInfo> moveReportSection(
    String report,
    String section,
    int to,
  ) async => _section(
    await r_reports.moveReportSection(
      report: report,
      section: section,
      to: PlatformInt64Util.from(to),
    ),
  );

  CodeSpaceInfo _codeSpace(r_code.CodeSpaceInfo c) => CodeSpaceInfo(
    id: c.id,
    shortId: c.shortId,
    name: c.name,
    description: c.description,
    lang: c.lang,
    source: c.source,
    input: c.input,
    scope: c.scope,
    approved: c.approved,
    trustHash: c.trustHash,
    lastStatus: c.lastStatus,
    lastError: c.lastError,
    lastAt: c.lastAt.toInt(),
    lastMs: c.lastMs.toInt(),
    lastWrites: c.lastWrites.toInt(),
  );

  CodeRunResult _codeRun(r_code.CodeRunResult r) => CodeRunResult(
    status: r.status,
    error: r.error,
    logs: r.logs,
    lines: r.lines,
    writes: r.writes.toInt(),
    ms: r.ms.toInt(),
  );

  @override
  Future<List<CodeSpaceInfo>> codeSpaces() async => [
    for (final c in await r_code.codeSpaces()) _codeSpace(c),
  ];

  @override
  Future<CodeSpaceInfo> codeCreate(
    String name, {
    String lang = 'math',
    String description = '',
    String? source,
    String? from,
    String? input,
  }) async => _codeSpace(
    await r_code.codeCreate(
      name: name,
      lang: lang,
      description: description,
      source: source,
      from: from,
      input: input,
    ),
  );

  @override
  Future<CodeSpaceInfo> codeEdit(
    String space, {
    String? name,
    String? description,
    String? lang,
    String? source,
    String? input,
  }) async => _codeSpace(
    await r_code.codeEdit(
      space: space,
      name: name,
      description: description,
      lang: lang,
      source: source,
      input: input,
    ),
  );

  @override
  Future<String> codeRemove(String space) => r_code.codeRemove(space: space);

  @override
  Future<String> codeApprove(String space) => r_code.codeApprove(space: space);

  @override
  Future<List<String>> codeCheck(String space) =>
      r_code.codeCheck(space: space);

  @override
  Future<CodeRunResult> codeBuild(String space) async =>
      _codeRun(await r_code.codeBuild(space: space));

  @override
  Future<CodeRunResult> codeRun(String space) async =>
      _codeRun(await r_code.codeRun(space: space));

  @override
  Future<List<CodeParserInfo>> codeParsers() async => [
    for (final p in await r_code.codeParsers())
      CodeParserInfo(name: p.name, about: p.about, installed: p.installed),
  ];

  @override
  Future<String> codeParsersInstall({String? name}) =>
      r_code.codeParsersInstall(name: name);

  LiveHopInfo _hop(r_code.LiveHopInfo h) => LiveHopInfo(
    name: h.name,
    status: h.status,
    rows: h.rows.toInt(),
    ms: h.ms.toInt(),
    columns: h.columns,
    error: h.error,
    state: h.state,
    rowsIn: h.rowsIn.toInt(),
  );

  @override
  Future<List<LivePipeInfo>> pipesLive() async => [
    for (final p in await r_code.pipesLive())
      LivePipeInfo(
        token: p.token,
        origin: p.origin,
        status: p.status,
        pipeline: p.pipeline,
        hops: [for (final h in p.hops) _hop(h)],
        saved: p.saved,
        updated: p.updated.toInt(),
      ),
  ];

  @override
  Future<PipeRunResult> pipeRun(
    String source, {
    List<String> steps = const [],
    List<String> to = const [],
  }) async {
    final r = await r_code.pipeRun(source: source, steps: steps, to: to);
    return PipeRunResult(hops: r.hops, saved: r.saved, lines: r.lines);
  }

  PipelineInfo _pipeline(r_code.PipelineInfo p) => PipelineInfo(
    id: p.id,
    shortId: p.shortId,
    name: p.name,
    description: p.description,
    version: p.version.toInt(),
    draft: p.draft,
    nodes: p.nodes.toInt(),
    sinks: p.sinks,
    problems: p.problems,
    nodeSpecs: p.nodeSpecs,
    edgeSpecs: p.edgeSpecs,
    graphLines: p.graphLines,
  );

  @override
  Future<List<PipelineInfo>> pipelines() async => [
    for (final p in await r_code.pipelines()) _pipeline(p),
  ];

  @override
  Future<PipelineInfo> pipelineCreate(
    String name, {
    String description = '',
    List<String> nodes = const [],
    List<String> edges = const [],
  }) async => _pipeline(
    await r_code.pipelineCreate(
      name: name,
      description: description,
      nodes: nodes,
      edges: edges,
    ),
  );

  @override
  Future<PipelineInfo> pipelineEdit(
    String pipeline, {
    String? name,
    String? description,
    List<String> nodes = const [],
    List<String> edges = const [],
    List<String> removeNodes = const [],
    List<String> removeEdges = const [],
  }) async => _pipeline(
    await r_code.pipelineEdit(
      pipeline: pipeline,
      name: name,
      description: description,
      nodes: nodes,
      edges: edges,
      removeNodes: removeNodes,
      removeEdges: removeEdges,
    ),
  );

  @override
  Future<String> pipelineRemove(String pipeline) =>
      r_code.pipelineRemove(pipeline: pipeline);

  @override
  Future<String> pipelineApprove(String pipeline) =>
      r_code.pipelineApprove(pipeline: pipeline);

  @override
  Future<PipelineRunResult> pipelineRun(String pipeline) async {
    final r = await r_code.pipelineRun(pipeline: pipeline);
    return PipelineRunResult(ok: r.ok, lines: r.lines, landed: r.landed);
  }

  @override
  Future<String> pipelineExport(
    String pipeline,
    String path, {
    bool templates = true,
  }) => r_code.pipelineExport(
    pipeline: pipeline,
    path: path,
    templates: templates,
  );

  @override
  Future<List<String>> pipelineImportPlan(
    String path, {
    String? name,
    bool merge = false,
  }) => r_code.pipelineImportPlan(path: path, name: name, merge: merge);

  @override
  Future<String> pipelineImport(
    String path, {
    String? name,
    bool merge = false,
  }) => r_code.pipelineImport(path: path, name: name, merge: merge);

  RuleInfo _rule(r_rules.RuleInfo r) => RuleInfo(
    id: r.id,
    shortId: r.shortId,
    name: r.name,
    description: r.description,
    enabled: r.enabled,
    home: r.home,
    brief: r.brief,
    onText: r.onText,
    whenText: r.whenText,
    thenText: r.thenText,
    related: r.related,
    runs: r.runs.toInt(),
    lastStatus: r.lastStatus,
    lastRunAt: r.lastRunAt.toInt(),
    nextRun: r.nextRun.toInt(),
    createdAt: r.createdAt.toInt(),
  );

  @override
  Future<List<RuleInfo>> rules() async => [
    for (final r in await r_rules.rules()) _rule(r),
  ];

  @override
  Future<RuleInfo> ruleCreate(
    String name, {
    String description = '',
    String on = '',
    String when = '',
    String then = '',
    String home = '',
    bool related = false,
  }) async => _rule(
    await r_rules.ruleCreate(
      name: name,
      description: description,
      on_: on,
      when: when,
      then: then,
      home: home,
      related: related,
    ),
  );

  @override
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
  }) async => _rule(
    await r_rules.ruleEdit(
      rule: rule,
      name: name,
      description: description,
      on_: on,
      when: when,
      then: then,
      home: home,
      related: related,
      noSchedule: noSchedule,
    ),
  );

  @override
  Future<String> ruleRemove(String rule) => r_rules.ruleRemove(rule: rule);

  @override
  Future<String> ruleEnable(String rule) => r_rules.ruleEnable(rule: rule);

  @override
  Future<String> ruleDisable(String rule) => r_rules.ruleDisable(rule: rule);

  @override
  Future<List<String>> ruleShow(String rule) => r_rules.ruleShow(rule: rule);

  @override
  Future<List<String>> ruleTest({String? rule, int? action}) =>
      r_rules.ruleTest(
        rule: rule,
        action: action == null ? null : PlatformInt64Util.from(action),
      );

  @override
  Future<List<String>> ruleDryRun({String? rule, int? action}) =>
      r_rules.ruleDryRun(
        rule: rule,
        action: action == null ? null : PlatformInt64Util.from(action),
      );

  @override
  Future<String> ruleTick() => r_rules.ruleTick();

  @override
  Future<List<RuleEventInfo>> ruleEvents({String? kind}) async => [
    for (final e in await r_rules.ruleEvents(kind: kind))
      RuleEventInfo(kind: e.kind, about: e.about, fields: e.fields),
  ];

  NotificationInfo _note(r_notify.NotificationInfo n) => NotificationInfo(
    id: n.id,
    shortId: n.shortId,
    title: n.title,
    kind: n.kind,
    level: n.level,
    body: n.body,
    link: n.link,
    linkLabel: n.linkLabel,
    actionId: n.actionId.toInt(),
    createdAt: n.createdAt.toInt(),
    read: n.read,
    readAt: n.readAt.toInt(),
  );

  @override
  Future<List<NotificationInfo>> notifications({
    bool unread = false,
    int limit = 50,
  }) async => [
    for (final n in await r_notify.notifications(
      unread: unread,
      limit: PlatformInt64Util.from(limit),
    ))
      _note(n),
  ];

  @override
  Future<int> notificationsUnreadCount() async =>
      (await r_notify.notificationsUnreadCount()).toInt();

  @override
  Future<NotificationInfo> notificationShow(String id) async =>
      _note(await r_notify.notificationShow(id: id));

  @override
  Future<String> notificationRead(String id) =>
      r_notify.notificationRead(id: id);

  @override
  Future<String> notificationUnread(String id) =>
      r_notify.notificationUnread(id: id);

  @override
  Future<String> notificationReadAll() => r_notify.notificationReadAll();

  @override
  Future<NotificationInfo> notificationSend(
    String title, {
    String body = '',
    String? link,
    String level = 'info',
  }) async => _note(
    await r_notify.notificationSend(
      title: title,
      body: body,
      link: link,
      level: level,
    ),
  );

  @override
  Future<List<NotificationKindInfo>> notificationKinds() async => [
    for (final k in await r_notify.notificationKinds())
      NotificationKindInfo(kind: k.kind, about: k.about, muted: k.muted),
  ];

  @override
  Future<String> notificationMute(String kind) =>
      r_notify.notificationMute(kind: kind);

  @override
  Future<String> notificationUnmute(String kind) =>
      r_notify.notificationUnmute(kind: kind);

  ReviewInfo _rev(r_reviews.ReviewInfo r) => ReviewInfo(
    id: r.id,
    shortId: r.shortId,
    title: r.title,
    kind: r.kind,
    name: r.name,
    note: r.note,
    status: r.status,
    allow: r.allow,
    items: r.items.toInt(),
    votesApprove: r.votesApprove.toInt(),
    votesChanges: r.votesChanges.toInt(),
    votesReject: r.votesReject.toInt(),
    createdAt: r.createdAt.toInt(),
    updatedAt: r.updatedAt.toInt(),
  );

  ReviewItemInfo _rit(r_reviews.ReviewItemInfo i) => ReviewItemInfo(
    id: i.id,
    shortId: i.shortId,
    line: i.line,
    kind: i.kind,
    who: i.who,
    state: i.state,
    field: i.field,
    target: i.target,
    quote: i.quote,
    body: i.body,
    value: i.value,
    createdAt: i.createdAt.toInt(),
  );

  ReviewDetail _detail(r_reviews.ReviewDetail d) => ReviewDetail(
    review: _rev(d.review),
    mine: d.mine,
    note: d.note,
    youMay: d.youMay,
    peer: d.peer,
    reviewers: [
      for (final v in d.reviewers)
        ReviewerInfo(
          id: v.id,
          shortId: v.shortId,
          title: v.title,
          status: v.status,
          label: v.label,
        ),
    ],
    items: [for (final i in d.items) _rit(i)],
  );

  @override
  Future<List<ReviewInfo>> reviews() async => [
    for (final r in await r_reviews.reviews()) _rev(r),
  ];

  @override
  Future<List<ReviewingInfo>> reviewsJoined() async => [
    for (final g in await r_reviews.reviewsJoined())
      ReviewingInfo(
        id: g.id,
        shortId: g.shortId,
        title: g.title,
        kind: g.kind,
        name: g.name,
        status: g.status,
        allow: g.allow,
        peer: g.peer,
        items: g.items.toInt(),
        updatedAt: g.updatedAt.toInt(),
      ),
  ];

  @override
  Future<ReviewInfo> reviewRequest(
    String kind,
    String target, {
    String title = '',
    String note = '',
    List<String> allow = const [],
  }) async => _rev(
    await r_reviews.reviewRequest(
      kind: kind,
      target: target,
      title: title,
      note: note,
      allow: allow,
    ),
  );

  @override
  Future<ReviewDetail> reviewShow(String review) async =>
      _detail(await r_reviews.reviewShow(review: review));

  @override
  Future<List<String>> reviewView(String review) =>
      r_reviews.reviewView(review: review);

  @override
  Future<String> reviewInvite(
    String review, {
    String label = '',
    String? peer,
  }) => r_reviews.reviewInvite(review: review, label: label, peer: peer);

  @override
  Future<String> reviewJoin(String token, {String label = ''}) =>
      r_reviews.reviewJoin(token: token, label: label);

  @override
  Future<String> reviewSync(String review) =>
      r_reviews.reviewSync(review: review);

  @override
  Future<String> reviewComment(
    String review,
    String text, {
    String? field,
    String? at,
    int nth = 1,
    String? entry,
    String? reply,
  }) => r_reviews.reviewComment(
    review: review,
    text: text,
    field: field,
    at: at,
    nth: PlatformInt64Util.from(nth),
    entry: entry,
    reply: reply,
  );

  @override
  Future<String> reviewSuggest(
    String review,
    String to, {
    String why = '',
    String? field,
    String? at,
    int nth = 1,
    String? entry,
  }) => r_reviews.reviewSuggest(
    review: review,
    to: to,
    why: why,
    field: field,
    at: at,
    nth: PlatformInt64Util.from(nth),
    entry: entry,
  );

  @override
  Future<String> reviewHighlight(
    String review, {
    String? field,
    String? at,
    int nth = 1,
    String? entry,
    String? color,
    String note = '',
  }) => r_reviews.reviewHighlight(
    review: review,
    field: field,
    at: at,
    nth: PlatformInt64Util.from(nth),
    entry: entry,
    color: color,
    note: note,
  );

  @override
  Future<String> reviewVote(
    String review,
    String vote, {
    String? on,
    String note = '',
  }) => r_reviews.reviewVote(review: review, vote: vote, on_: on, note: note);

  @override
  Future<String> reviewWithdraw(String review, String item) =>
      r_reviews.reviewWithdraw(review: review, item: item);

  @override
  Future<String> reviewAccept(
    String review, {
    List<String> items = const [],
    bool all = false,
  }) => r_reviews.reviewAccept(review: review, items: items, all: all);

  @override
  Future<String> reviewReject(
    String review, {
    List<String> items = const [],
    bool all = false,
  }) => r_reviews.reviewReject(review: review, items: items, all: all);

  @override
  Future<String> reviewMerge(String review, {bool force = false}) =>
      r_reviews.reviewMerge(review: review, force: force);

  @override
  Future<String> reviewClose(String review) =>
      r_reviews.reviewClose(review: review);

  @override
  Future<String> reviewReopen(String review) =>
      r_reviews.reviewReopen(review: review);

  @override
  Future<String> reviewDrop(String review, String reviewer) =>
      r_reviews.reviewDrop(review: review, reviewer: reviewer);

  @override
  Future<String> reviewLeave(String review) =>
      r_reviews.reviewLeave(review: review);

  @override
  Future<String> reviewRemove(String review) =>
      r_reviews.reviewRemove(review: review);

  @override
  Future<List<String>> reviewRevisions(String kind, String target) =>
      r_reviews.reviewRevisions(kind: kind, target: target);

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

  NetConfig _netConfig(r_net.NetConfig c) => NetConfig(
    label: c.label,
    port: c.port,
    listen: c.listen,
    mdns: c.mdns,
    relayServer: c.relayServer,
    autostart: c.autostart,
    hostAutostart: c.hostAutostart,
    accept: c.accept,
    bootstrap: c.bootstrap,
    externalAddresses: c.externalAddresses,
    collabTransport: c.collabTransport,
    collabBind: c.collabBind,
    collabPort: c.collabPort,
    collabAdvertise: c.collabAdvertise,
  );

  @override
  Future<NetConfig> netConfig() async =>
      _netConfig(await r_net.netConfig(home: home));

  @override
  Future<String> netConfigSet(NetConfig cfg) => r_net.netConfigSet(
    home: home,
    cfg: r_net.NetConfig(
      label: cfg.label,
      port: cfg.port,
      listen: cfg.listen,
      mdns: cfg.mdns,
      relayServer: cfg.relayServer,
      autostart: cfg.autostart,
      hostAutostart: cfg.hostAutostart,
      accept: cfg.accept,
      bootstrap: cfg.bootstrap,
      externalAddresses: cfg.externalAddresses,
      collabTransport: cfg.collabTransport,
      collabBind: cfg.collabBind,
      collabPort: cfg.collabPort,
      collabAdvertise: cfg.collabAdvertise,
    ),
  );

  NetConnNode _netConnNode(r_net.NetConnNode n) => NetConnNode(
    shortId: n.shortId,
    label: n.label,
    active: n.active,
    state: n.state,
  );

  NetConnectionInfo _netConnection(r_net.NetConnectionInfo c) =>
      NetConnectionInfo(
        userId: c.userId,
        shortId: c.shortId,
        name: c.name,
        trusted: c.trusted,
        nodes: c.nodes.toInt(),
        since: c.since.toInt(),
        nodeList: [for (final n in c.nodeList) _netConnNode(n)],
      );

  @override
  Future<NetConnections> netConnections() async {
    final o = await r_net.netConnections(home: home);
    return NetConnections(
      me: o.me == null
          ? null
          : NetMeInfo(
              userId: o.me!.userId,
              shortId: o.me!.shortId,
              name: o.me!.name,
              nodes: o.me!.nodes.toInt(),
            ),
      connections: [for (final c in o.connections) _netConnection(c)],
    );
  }

  @override
  Future<String> netConnectionTrust(String user, bool trusted) =>
      r_net.netConnectionTrust(home: home, user: user, trusted: trusted);

  @override
  Future<String> netConnectionRemove(String user) =>
      r_net.netConnectionRemove(home: home, user: user);

  NetExchangeInfo _netExchange(r_net.NetExchangeInfo x) => NetExchangeInfo(
    id: x.id,
    shortId: x.shortId,
    title: x.title,
    label: x.label,
    status: x.status,
    role: x.role,
    peerId: x.peerId,
    peerLabel: x.peerLabel,
    shortCode: x.shortCode,
    fingerprint: x.fingerprint,
    expiresAt: x.expiresAt?.toInt(),
    verifiedAt: x.verifiedAt?.toInt(),
    revokedAt: x.revokedAt?.toInt(),
    createdAt: x.createdAt.toInt(),
    active: x.active,
    incoming: x.incoming,
  );

  @override
  Future<List<NetExchangeInfo>> netExchanges() async => [
    for (final x in await r_net.netExchanges(home: home)) _netExchange(x),
  ];

  @override
  Future<String> netExchangeLabel(String exchange, String label) =>
      r_net.netExchangeLabel(home: home, exchange: exchange, label: label);

  @override
  Future<String> netExchangeVerify(String exchange, String code) =>
      r_net.netExchangeVerify(home: home, exchange: exchange, code: code);

  @override
  Future<String> netExchangeRevoke(String exchange) =>
      r_net.netExchangeRevoke(home: home, exchange: exchange);

  @override
  Future<List<NetRemoteItem>> netRemoteList(String exchange) async => [
    for (final i in await r_net.netRemoteList(home: home, exchange: exchange))
      NetRemoteItem(id: i.id, kind: i.kind, name: i.name, about: i.about),
  ];

  @override
  Future<String> netFetch(
    String exchange,
    String item, {
    String? format,
    String? path,
    bool import = false,
  }) => r_net.netFetch(
    home: home,
    exchange: exchange,
    item: item,
    format: format,
    path: path,
    import_: import,
  );

  @override
  Future<NetShards> netShards() async {
    final s = await r_net.netShards(home: home);
    return NetShards(
      entrusted: [
        for (final e in s.entrusted)
          NetEntrustedInfo(
            setId: e.setId,
            index: e.index,
            threshold: e.threshold,
            total: e.total,
            exchange: e.exchange,
            status: e.status,
          ),
      ],
      held: [
        for (final h in s.held)
          NetHeldInfo(
            id: h.id,
            shortId: h.shortId,
            setId: h.setId,
            index: h.index,
            threshold: h.threshold,
            total: h.total,
            owner: h.owner,
            exchange: h.exchange,
          ),
      ],
    );
  }

  @override
  Future<String> netShardEntrust({
    required List<String> to,
    required String keyFile,
    required String shardPassword,
    String? filePassword,
    int threshold = 0,
  }) => r_net.netShardEntrust(
    home: home,
    to: to,
    threshold: threshold,
    keyFile: keyFile,
    filePassword: filePassword,
    shardPassword: shardPassword,
  );

  @override
  Future<String> netShardCheck(String shardPassword) =>
      r_net.netShardCheck(home: home, shardPassword: shardPassword);

  @override
  Future<String> netShardDrop(String id) =>
      r_net.netShardDrop(home: home, id: id);

  @override
  Future<List<NetSearchOrigin>> netSearch(
    String saved, {
    List<String> only = const [],
    bool remoteOnly = false,
    int limit = 200,
  }) async => [
    for (final o in await r_net.netSearch(
      home: home,
      saved: saved,
      only: only,
      remoteOnly: remoteOnly,
      limit: limit,
    ))
      NetSearchOrigin(
        origin: o.origin,
        peer: o.peer,
        exchange: o.exchange,
        error: o.error,
        truncated: o.truncated,
        lines: o.lines,
      ),
  ];

  @override
  Future<String> netBroadcast(
    String topic,
    String text, {
    String to = 'nodes',
  }) => r_net.netBroadcast(home: home, topic: topic, text: text, to: to);

  @override
  Future<String> netExchangeExport({
    required String out,
    required String password,
  }) => r_net.netExchangeExport(home: home, out: out, password: password);

  @override
  Future<NetExchangeBackupPeek> netExchangeRestorePeek({
    required String file,
    required String password,
  }) async {
    final p = await r_net.netExchangeRestorePeek(
      home: home,
      file: file,
      password: password,
    );
    return NetExchangeBackupPeek(
      peerId: p.peerId,
      exchanges: p.exchanges,
      identityReplaced: p.identityReplaced,
    );
  }

  @override
  Future<String> netExchangeRestore({
    required String file,
    required String password,
    bool confirmReplace = false,
  }) => r_net.netExchangeRestore(
    home: home,
    file: file,
    password: password,
    confirmReplace: confirmReplace,
  );

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

  OrgTemplateInfo _orgTemplate(r_orgs.OrgTemplateInfo t) => OrgTemplateInfo(
    name: t.name,
    inUse: t.inUse,
    updatedAt: t.updatedAt,
    assetNames: t.assetNames,
    buildCmd: t.buildCmd,
    buildSrc: t.buildSrc,
    buildOut: t.buildOut,
    buildApproved: t.buildApproved,
    html: t.html,
    css: t.css,
  );

  @override
  Future<List<OrgTemplateInfo>> orgTemplates(String org) async => [
    for (final t in await r_orgs.orgTemplates(org: org)) _orgTemplate(t),
  ];

  @override
  Future<OrgTemplateInfo> orgTemplateShow(String org, String template) async =>
      _orgTemplate(await r_orgs.orgTemplateShow(org: org, template: template));

  @override
  Future<OrgTemplateInfo> orgTemplateSave(
    String org,
    String name, {
    String html = '',
    String css = '',
    bool useIt = false,
  }) async => _orgTemplate(
    await r_orgs.orgTemplateSave(
      org: org,
      name: name,
      html: html,
      css: css,
      useIt: useIt,
    ),
  );

  @override
  Future<String> orgTemplateRemove(String org, String template) =>
      r_orgs.orgTemplateRemove(org: org, template: template);

  @override
  Future<String> orgTemplateAddAsset(
    String org,
    String template,
    String path, {
    String? name,
  }) => r_orgs.orgTemplateAddAsset(
    org: org,
    template: template,
    path: path,
    name: name,
  );

  @override
  Future<String> orgTemplateRemoveAsset(
    String org,
    String template,
    String name,
  ) => r_orgs.orgTemplateRemoveAsset(org: org, template: template, name: name);

  @override
  Future<String> orgTemplateBuild(
    String org,
    String template, {
    String? cmd,
    String? src,
    String? out,
    bool clear = false,
    bool noRun = false,
    bool approve = false,
  }) => r_orgs.orgTemplateBuild(
    org: org,
    template: template,
    cmd: cmd,
    src: src,
    out: out,
    clear: clear,
    noRun: noRun,
    approve: approve,
  );

  @override
  Future<String> orgTemplatePreview(String org, String template) =>
      r_orgs.orgTemplatePreview(home: home, org: org, template: template);

  @override
  Future<List<OrgDomainInfo>> orgDomains({String? org}) async => [
    for (final d in await r_orgs.orgDomains(org: org))
      OrgDomainInfo(org: d.org, domain: d.domain),
  ];

  @override
  Future<String> orgDomainAdd(String org, String domain) =>
      r_orgs.orgDomainAdd(org: org, domain: domain);

  @override
  Future<String> orgDomainRemove(String org, String domain) =>
      r_orgs.orgDomainRemove(org: org, domain: domain);

  OrgCertOverview _orgCerts(r_orgs.OrgCertOverview s) => OrgCertOverview(
    node: OrgNodeCertInfo(
      source: s.node.source,
      names: s.node.names,
      notAfter: s.node.notAfter,
      issuer: s.node.issuer,
      acmeEnabled: s.node.acmeEnabled,
      acmeDomain: s.node.acmeDomain,
      acmeEmail: s.node.acmeEmail,
      acmeDirectory: s.node.acmeDirectory,
      acmeProfile: s.node.acmeProfile,
      acmeChallenge: s.node.acmeChallenge,
      acmeHttpPort: s.node.acmeHttpPort.toInt(),
      selfSignedDays: s.node.selfSignedDays.toInt(),
    ),
    orgs: [
      for (final c in s.orgs)
        OrgCertInfo(
          org: c.org,
          slug: c.slug,
          domains: c.domains,
          source: c.source,
          names: c.names,
          notAfter: c.notAfter,
          issuer: c.issuer,
          acmeEnabled: c.acmeEnabled,
          acmeEmail: c.acmeEmail,
          retryAttempts: c.retryAttempts.toInt(),
          retryNext: c.retryNext,
          retryError: c.retryError,
        ),
    ],
  );

  @override
  Future<OrgCertOverview> orgCertStatus({String? org}) async =>
      _orgCerts(await r_orgs.orgCertStatus(home: home, org: org));

  @override
  Future<String> orgCertSet(String org, String cert, String key) =>
      r_orgs.orgCertSet(org: org, cert: cert, key: key);

  @override
  Future<String> orgCertClear(String org) => r_orgs.orgCertClear(org: org);

  @override
  Future<String> orgCertAcme({
    String? org,
    String? challenge,
    String? email,
    String? domain,
    String? directory,
    String? profile,
    int? httpPort,
    bool off = false,
  }) => r_orgs.orgCertAcme(
    home: home,
    org: org,
    challenge: challenge,
    email: email,
    domain: domain,
    directory: directory,
    profile: profile,
    httpPort: httpPort == null ? null : PlatformInt64Util.from(httpPort),
    off: off,
  );

  @override
  Future<String> orgCertRenew({String? org}) =>
      r_orgs.orgCertRenew(home: home, org: org);

  @override
  Future<List<OrgBrowseItem>> orgBrowse(String target, {String? org}) async => [
    for (final c in await r_orgs.orgBrowse(
      home: home,
      target: target,
      org: org,
    ))
      OrgBrowseItem(
        name: c.name,
        slug: c.slug,
        visibility: c.visibility,
        access: c.access,
        members: c.members.toInt(),
        description: c.description,
      ),
  ];

  @override
  Future<OrgFetchResult> orgFetch(
    String org,
    String resource, {
    String? format,
    String? out,
  }) async {
    final r = await r_orgs.orgFetch(
      home: home,
      org: org,
      resource: resource,
      format: format,
      out: out,
    );
    return OrgFetchResult(
      name: r.name,
      format: r.format,
      size: r.size.toInt(),
      text: r.text,
      written: r.written,
    );
  }

  @override
  Future<String> orgGive(String org, String kind, String name) =>
      r_orgs.orgGive(home: home, org: org, kind: kind, name: name);

  @override
  Future<String> orgMirror(String org) =>
      r_orgs.orgMirror(home: home, org: org);

  @override
  Future<List<OrgSyncCodeItem>> orgSyncCode(String org) async => [
    for (final x in await r_orgs.orgSyncCode(home: home, org: org))
      OrgSyncCodeItem(name: x.name, state: x.state),
  ];

  @override
  Future<String> orgSignIn(String org, String site) =>
      r_orgs.orgSignIn(org: org, site: site);

  @override
  Future<String> orgEndpointAdd(
    String org,
    String name,
    String kind,
    String target, {
    String? perm,
    bool anyone = false,
  }) => r_orgs.orgEndpointAdd(
    org: org,
    name: name,
    kind: kind,
    target: target,
    perm: perm,
    anyone: anyone,
  );

  @override
  Future<String> orgEndpointRemove(String org, String name) =>
      r_orgs.orgEndpointRemove(org: org, name: name);

  @override
  Future<String> orgCall(String org, String endpoint, {String input = '{}'}) =>
      r_orgs.orgCall(home: home, org: org, endpoint: endpoint, input: input);

  @override
  Future<String> orgProjectAdd(String org, String project) =>
      r_orgs.orgProjectAdd(org: org, project: project);

  @override
  Future<String> orgProjectRemove(String org, String project) =>
      r_orgs.orgProjectRemove(org: org, project: project);

  @override
  Future<String> orgProjectMember(
    String org,
    String project,
    String member, {
    String? role,
  }) => r_orgs.orgProjectMember(
    org: org,
    project: project,
    member: member,
    role: role,
  );

  @override
  Future<List<OrgCodeServedInfo>> orgCodeList(String org) async => [
    for (final c in await r_orgs.orgCodeList(org: org))
      OrgCodeServedInfo(name: c.name, lang: c.lang, shortId: c.shortId),
  ];

  @override
  Future<String> orgCodeServe(String org, String space) =>
      r_orgs.orgCodeServe(org: org, space: space);

  @override
  Future<String> orgCodeStop(String org, String space) =>
      r_orgs.orgCodeStop(org: org, space: space);

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

JournalAnnotationInfo _ann(r_journals.JournalAnnotationInfo a) =>
    JournalAnnotationInfo(
      id: a.id,
      shortId: a.shortId,
      journalId: a.journalId,
      entryId: a.entryId,
      kind: a.kind,
      start: a.start?.toInt(),
      end: a.end?.toInt(),
      quote: a.quote,
      body: a.body,
      color: a.color,
      detached: a.detached,
    );

JournalImageInfo _image(r_journals.JournalImageInfo i) => JournalImageInfo(
  id: i.id,
  shortId: i.shortId,
  entryId: i.entryId,
  name: i.name,
  mime: i.mime,
  size: i.size.toInt(),
);

JournalExportInfo _jexport(r_journals.JournalExportInfo e) =>
    JournalExportInfo(path: e.path, files: e.files.toInt(), message: e.message);

JournalInfo _journal(r_journals.JournalInfo j) => JournalInfo(
  id: j.id,
  shortId: j.shortId,
  name: j.name,
  projectId: j.projectId,
  projectName: j.projectName,
  format: j.format,
  entries: j.entries.toInt(),
  forkedFrom: j.forkedFrom,
);

JournalEntryInfo _entry(r_journals.EntryInfo e) => JournalEntryInfo(
  id: e.id,
  shortId: e.shortId,
  journalId: e.journalId,
  title: e.title,
  heading: e.heading,
  body: e.body,
  format: e.format,
  position: e.position.toInt(),
);

SearchInfo _search(r_searches.SearchInfo s) => SearchInfo(
  id: s.id,
  shortId: s.shortId,
  name: s.name,
  target: s.target,
  query: s.query,
  describe: s.describe,
  matches: s.matches.toInt(),
  forkedFrom: s.forkedFrom,
  sources: s.sources,
  symbols: s.symbols,
  periods: s.periods,
  years: List<int>.from(s.years),
  from: s.from?.toInt(),
  to: s.to?.toInt(),
  priceMetric: s.priceMetric,
  priceMin: s.priceMin,
  priceMax: s.priceMax,
  sort: s.sort,
  desc: s.desc,
  model: s.model,
  conditions: s.conditions,
);

ReportInfo _report(r_reports.ReportInfo r) => ReportInfo(
  id: r.id,
  shortId: r.shortId,
  name: r.name,
  description: r.description,
  sections: r.sections.toInt(),
);

ReportSectionInfo _section(r_reports.SectionInfo s) => ReportSectionInfo(
  id: s.id,
  shortId: s.shortId,
  reportId: s.reportId,
  position: s.position.toInt(),
  kind: s.kind,
  title: s.title,
  heading: s.heading,
  body: s.body,
  format: s.format,
);
