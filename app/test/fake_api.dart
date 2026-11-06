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
  final itemsByProject = <String, List<ProjectItemInfo>>{};
  List<JournalInfo> journalList = [];
  List<JournalEntryInfo> entryList = [];
  List<JournalNoteInfo> noteList = [];
  List<SearchInfo> searchList = [];
  List<ReportInfo> reportList = [];
  List<ReportSectionInfo> sectionList = [];
  final extraCalls = <String>[];
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
    itemsByProject.remove(id);
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
    for (final id in itemsByProject.keys.toList()) {
      itemsByProject[id] = [
        for (final i in itemsByProject[id]!)
          if (i.itemId != itemId) i,
      ];
    }
  }

  @override
  Future<List<ProjectItemInfo>> projectItems(String projectId) async {
    final series = [
      for (final s in seriesByProject[projectId] ?? const <ProjectSeries>[])
        ProjectItemInfo(
          itemId: s.itemId,
          kind: 'result',
          name: '${s.source} · ${s.base}/${s.quote}',
          source: s.source,
          base: s.base,
          quote: s.quote,
          durationS: s.durationS,
          from: s.from,
          to: s.to,
        ),
    ];
    return [...series, ...?itemsByProject[projectId]];
  }

  @override
  Future<String> addProjectItem(String project, String kind, String key) async {
    extraCalls.add('add-item:$project:$kind:$key');
    final list = List<ProjectItemInfo>.from(
      itemsByProject[project] ?? const [],
    );
    list.add(
      ProjectItemInfo(
        itemId: nextItemId++,
        kind: kind == 'search' ? 'live_search' : kind,
        name: key,
        refId: key,
      ),
    );
    itemsByProject[project] = list;
    return 'added $kind $key to $project';
  }

  @override
  Future<String> projectImport(String path, {String? name}) async {
    extraCalls.add('import:$path');
    final p = Project(
      id: 'p$nextProject',
      shortId: 'imp$nextProject',
      name: name ?? 'Imported',
      description: '',
    );
    nextProject++;
    projects = [...projects, p];
    seriesByProject[p.id] = [];
    return 'imported project ${p.name} from $path';
  }

  @override
  Future<String> projectExport(
    String id,
    String path, {
    String format = 'archive',
    bool noData = false,
  }) async {
    extraCalls.add('export:$id:$path:$format');
    return 'exported project $id to $path';
  }

  @override
  Future<List<JournalInfo>> journals() async => List.of(journalList);

  @override
  Future<JournalInfo> createJournal(
    String name, {
    String? project,
    String format = 'markdown',
  }) async {
    extraCalls.add('journal-create:$name');
    final j = JournalInfo(
      id: 'j${journalList.length + 1}',
      shortId: 'j${journalList.length + 1}',
      name: name,
      projectId: project,
      format: format,
    );
    journalList = [...journalList, j];
    if (project != null) {
      await addProjectItem(project, 'journal', j.name);
    }
    return j;
  }

  @override
  Future<JournalInfo> updateJournal(
    String id, {
    String? name,
    String? format,
  }) async {
    extraCalls.add('journal-edit:$id');
    journalList = [
      for (final j in journalList)
        if (j.id == id)
          JournalInfo(
            id: j.id,
            shortId: j.shortId,
            name: name ?? j.name,
            projectId: j.projectId,
            projectName: j.projectName,
            format: format ?? j.format,
            entries: j.entries,
            forkedFrom: j.forkedFrom,
          )
        else
          j,
    ];
    return journalList.firstWhere((j) => j.id == id);
  }

  @override
  Future<String> removeJournal(String id) async {
    extraCalls.add('journal-remove:$id');
    final name = journalList.firstWhere((j) => j.id == id).name;
    journalList = [
      for (final j in journalList)
        if (j.id != id) j,
    ];
    return 'removed journal $name';
  }

  @override
  Future<JournalInfo> forkJournal(
    String id, {
    String? project,
    String? name,
  }) async {
    extraCalls.add('journal-fork:$id');
    final j = journalList.firstWhere((x) => x.id == id);
    final f = JournalInfo(
      id: 'j${journalList.length + 1}',
      shortId: 'jf',
      name: name ?? '${j.name} (fork)',
      projectId: project,
    );
    journalList = [...journalList, f];
    return f;
  }

  @override
  Future<List<JournalEntryInfo>> journalEntries(String journal) async => [
    for (final e in entryList)
      if (e.journalId == journal) e,
  ];

  @override
  Future<JournalEntryInfo> addJournalEntry(
    String journal, {
    String title = '',
    String body = '',
    String? format,
  }) async {
    extraCalls.add('entry-add:$journal');
    final e = JournalEntryInfo(
      id: 'e${entryList.length + 1}',
      shortId: 'e${entryList.length + 1}',
      journalId: journal,
      title: title,
      heading: title.isEmpty ? body : title,
      body: body,
      format: format ?? 'markdown',
    );
    entryList = [...entryList, e];
    return e;
  }

  @override
  Future<JournalEntryInfo> updateJournalEntry(
    String entry, {
    String? title,
    String? body,
    String? format,
  }) async {
    extraCalls.add('entry-edit:$entry');
    entryList = [
      for (final e in entryList)
        if (e.id == entry)
          JournalEntryInfo(
            id: e.id,
            shortId: e.shortId,
            journalId: e.journalId,
            title: title ?? e.title,
            heading: title ?? e.heading,
            body: body ?? e.body,
            format: format ?? e.format,
            position: e.position,
          )
        else
          e,
    ];
    return entryList.firstWhere((e) => e.id == entry);
  }

  @override
  Future<String> removeJournalEntry(String entry) async {
    extraCalls.add('entry-remove:$entry');
    entryList = [
      for (final e in entryList)
        if (e.id != entry) e,
    ];
    return 'removed entry $entry';
  }

  @override
  Future<List<JournalNoteInfo>> journalNotes(String journal) async =>
      List.of(noteList);

  @override
  Future<JournalNoteInfo> addJournalNote(String journal, String body) async {
    extraCalls.add('note:$journal');
    final n = JournalNoteInfo(
      id: 'n${noteList.length + 1}',
      shortId: 'n${noteList.length + 1}',
      body: body,
    );
    noteList = [...noteList, n];
    return n;
  }

  @override
  Future<List<SearchInfo>> searches() async => List.of(searchList);

  @override
  Future<SearchInfo> saveSearch(
    String name, {
    String query = '',
    String target = 'series',
  }) async {
    extraCalls.add('search-save:$name');
    final s = SearchInfo(
      id: 's${searchList.length + 1}',
      shortId: 's${searchList.length + 1}',
      name: name,
      query: query,
      target: target,
      describe: '$target "$query"',
    );
    searchList = [...searchList, s];
    return s;
  }

  @override
  Future<SearchInfo> renameSearch(String id, String name) async {
    extraCalls.add('search-rename:$id');
    searchList = [
      for (final s in searchList)
        if (s.id == id)
          SearchInfo(
            id: s.id,
            shortId: s.shortId,
            name: name,
            target: s.target,
            query: s.query,
            describe: s.describe,
            matches: s.matches,
            forkedFrom: s.forkedFrom,
          )
        else
          s,
    ];
    return searchList.firstWhere((s) => s.id == id);
  }

  @override
  Future<SearchInfo> forkSearch(String id, {String? name}) async {
    extraCalls.add('search-fork:$id');
    final s = searchList.firstWhere((x) => x.id == id);
    final f = SearchInfo(
      id: 's${searchList.length + 1}',
      shortId: 'sf',
      name: name ?? '${s.name} (fork)',
      target: s.target,
      query: s.query,
      describe: s.describe,
    );
    searchList = [...searchList, f];
    return f;
  }

  @override
  Future<String> removeSearch(String id) async {
    extraCalls.add('search-remove:$id');
    searchList = [
      for (final s in searchList)
        if (s.id != id) s,
    ];
    return 'removed live search $id';
  }

  @override
  Future<List<ReportInfo>> reports() async => List.of(reportList);

  @override
  Future<ReportInfo> createReport(
    String name, {
    String description = '',
    String? project,
  }) async {
    extraCalls.add('report-create:$name');
    final r = ReportInfo(
      id: 'r${reportList.length + 1}',
      shortId: 'r${reportList.length + 1}',
      name: name,
      description: description,
    );
    reportList = [...reportList, r];
    if (project != null) {
      await addProjectItem(project, 'report', r.name);
    }
    return r;
  }

  @override
  Future<ReportInfo> updateReport(
    String id, {
    String? name,
    String? description,
  }) async {
    extraCalls.add('report-edit:$id');
    reportList = [
      for (final r in reportList)
        if (r.id == id)
          ReportInfo(
            id: r.id,
            shortId: r.shortId,
            name: name ?? r.name,
            description: description ?? r.description,
            sections: r.sections,
          )
        else
          r,
    ];
    return reportList.firstWhere((r) => r.id == id);
  }

  @override
  Future<String> removeReport(String id) async {
    extraCalls.add('report-remove:$id');
    reportList = [
      for (final r in reportList)
        if (r.id != id) r,
    ];
    return 'removed report $id';
  }

  @override
  Future<List<ReportSectionInfo>> reportSections(String report) async => [
    for (final s in sectionList)
      if (s.reportId == report) s,
  ];

  @override
  Future<ReportSectionInfo> addReportSection(
    String report, {
    String kind = 'text',
    String title = '',
    String body = '',
    String format = 'markdown',
    String? target,
  }) async {
    extraCalls.add('section-add:$report:$kind');
    final s = ReportSectionInfo(
      id: 'sec${sectionList.length + 1}',
      shortId: 'sec${sectionList.length + 1}',
      reportId: report,
      position: sectionList.length + 1,
      kind: kind,
      title: title,
      heading: title.isEmpty ? kind : title,
      body: body,
      format: format,
    );
    sectionList = [...sectionList, s];
    return s;
  }

  @override
  Future<ReportSectionInfo> updateReportSection(
    String report,
    String section, {
    String? title,
    String? body,
    String? format,
  }) async {
    extraCalls.add('section-edit:$section');
    sectionList = [
      for (final s in sectionList)
        if (s.id == section)
          ReportSectionInfo(
            id: s.id,
            shortId: s.shortId,
            reportId: s.reportId,
            position: s.position,
            kind: s.kind,
            title: title ?? s.title,
            heading: title ?? s.heading,
            body: body ?? s.body,
            format: format ?? s.format,
          )
        else
          s,
    ];
    return sectionList.firstWhere((s) => s.id == section);
  }

  @override
  Future<String> removeReportSection(String report, String section) async {
    extraCalls.add('section-remove:$section');
    sectionList = [
      for (final s in sectionList)
        if (s.id != section) s,
    ];
    return 'removed section $section';
  }

  @override
  Future<ReportSectionInfo> moveReportSection(
    String report,
    String section,
    int to,
  ) async {
    extraCalls.add('section-move:$section:$to');
    sectionList = [
      for (final s in sectionList)
        if (s.id == section)
          ReportSectionInfo(
            id: s.id,
            shortId: s.shortId,
            reportId: s.reportId,
            position: to,
            kind: s.kind,
            title: s.title,
            heading: s.heading,
            body: s.body,
            format: s.format,
          )
        else
          s,
    ];
    return sectionList.firstWhere((s) => s.id == section);
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

  List<SourceInfo> sources = [
    const SourceInfo(id: 's-cb', name: 'coinbase', kind: 'csv', createdAt: 1),
    const SourceInfo(id: 's-bn', name: 'binance', kind: 'csv', createdAt: 2),
  ];
  final sourceData = <String, List<SourceDataItem>>{};
  List<SymbolInfo> symbols = [
    const SymbolInfo(symbol: 'BTC/USD', base: 'BTC', quote: 'USD'),
    const SymbolInfo(symbol: 'ETH/USDT', base: 'ETH', quote: 'USDT'),
  ];
  List<AliasInfo> aliases = [];
  int _nextSource = 3;
  final catalogCalls = <String>[];

  @override
  Future<List<SourceInfo>> listSources() async => List<SourceInfo>.from(sources);

  @override
  Future<SourceInfo> addSource(
    String name, {
    String kind = 'file',
    String? locator,
  }) async {
    catalogCalls.add('addSource $name $kind ${locator ?? ''}');
    if (name.trim().isEmpty || name.contains('|')) {
      throw 'source name `$name` (must be non-empty, no `|`)';
    }
    if (sources.any((s) => s.name == name)) {
      throw 'source name `$name` already exists';
    }
    final k = kind.trim().isEmpty ? 'file' : kind.trim();
    final s = SourceInfo(
      id: 's-$_nextSource',
      name: name.trim(),
      kind: k,
      locator: locator,
      createdAt: _nextSource,
    );
    _nextSource++;
    sources = [...sources, s];
    return s;
  }

  @override
  Future<int> renameSource(String from, String to) async {
    catalogCalls.add('renameSource $from $to');
    final i = sources.indexWhere((s) => s.name == from);
    if (i < 0) throw 'source `$from` not found';
    if (to.trim().isEmpty || to.contains('|')) {
      throw 'source name `$to`';
    }
    if (sources.any((s) => s.name == to)) {
      throw 'source name `$to` already exists';
    }
    final cur = sources[i];
    sources = [
      ...sources.sublist(0, i),
      SourceInfo(
        id: cur.id,
        name: to.trim(),
        kind: cur.kind,
        locator: cur.locator,
        createdAt: cur.createdAt,
        items: cur.items,
        bytes: cur.bytes,
      ),
      ...sources.sublist(i + 1),
    ];
    final data = sourceData.remove(from);
    if (data != null) sourceData[to.trim()] = data;
    return 0;
  }

  @override
  Future<int> removeSource(String name) async {
    catalogCalls.add('removeSource $name');
    final i = sources.indexWhere((s) => s.name == name);
    if (i < 0) throw 'source `$name` not found';
    sources = [...sources.sublist(0, i), ...sources.sublist(i + 1)];
    sourceData.remove(name);
    return 0;
  }

  @override
  Future<List<SourceDataItem>> listSourceData(String name) async {
    if (!sources.any((s) => s.name == name)) throw 'no source `$name`';
    return List<SourceDataItem>.from(sourceData[name] ?? const []);
  }

  @override
  Future<int> exportSourceData(String name, String item, String path) async {
    catalogCalls.add('exportSourceData $name $item $path');
    final items = await listSourceData(name);
    final found = items.where((i) => i.id.startsWith(item)).toList();
    if (found.isEmpty) throw 'no item `$item` in $name';
    if (found.length > 1) throw '`$item` matches ${found.length} items';
    return found.first.size;
  }

  @override
  Future<List<SymbolInfo>> listSymbols() async => List<SymbolInfo>.from(symbols);

  @override
  Future<List<AliasInfo>> listAliases() async => List<AliasInfo>.from(aliases);

  @override
  Future<SymbolInfo> addSymbol(
    String symbol, {
    List<String> aliases = const [],
  }) async {
    catalogCalls.add('addSymbol $symbol ${aliases.join(",")}');
    final t = symbol.trim().toUpperCase();
    String? base;
    String? quote;
    for (final sep in ['/', '-', '_', ':']) {
      if (t.contains(sep)) {
        final parts = t.split(sep);
        if (parts.length == 2 && parts[0].isNotEmpty && parts[1].isNotEmpty) {
          base = parts[0];
          quote = parts[1];
          break;
        }
      }
    }
    if (base == null || quote == null) {
      throw 'symbol `$symbol` (expected BASE/QUOTE)';
    }
    final info = SymbolInfo(symbol: '$base/$quote', base: base, quote: quote);
    if (!symbols.any((s) => s.symbol == info.symbol)) {
      symbols = [...symbols, info];
    }
    for (final a in aliases) {
      final alias = a.trim().toUpperCase();
      if (alias.isEmpty) continue;
      this.aliases = [
        ...this.aliases.where((x) => x.alias != alias),
        AliasInfo(alias: alias, canonical: info.base),
      ];
    }
    return info;
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
