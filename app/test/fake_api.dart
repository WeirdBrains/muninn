import 'dart:typed_data';

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
    tagline: 'Data goes in, your magic transforms it.',
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

  final Map<String, String> namedLayouts = {};
  bool historyBarOpen = false;
  List<HistoryRowInfo> historyList = const [];
  List<AuthorInfo> authorList = const [];
  List<FoundEntryInfo> foundEntries = const [];
  List<FoundRecordInfo> foundRecords = const [];
  List<ModelInfo> modelList = const [];
  List<RecordInfo> recordList = const [];
  List<ModelVersionInfo> modelVersions = const [];
  int _modelSeq = 0;
  int _fieldSeq = 0;
  int _recordSeq = 0;
  final Map<String, PipeFrameInfo> pipeFrames = {};
  String? liveLayoutJson;

  @override
  Future<List<String>> savedLayouts() async =>
      namedLayouts.keys.toList()..sort();

  @override
  Future<String> saveNamedLayout(String name, String json) async {
    extraCalls.add('layout-save:$name');
    namedLayouts[name] = json;
    return 'saved layout $name';
  }

  @override
  Future<String> namedLayout(String name) async {
    extraCalls.add('layout-load:$name');
    final json = namedLayouts[name];
    if (json == null) throw 'no saved layout `$name`';
    return json;
  }

  @override
  Future<String> removeNamedLayout(String name) async {
    extraCalls.add('layout-remove:$name');
    if (namedLayouts.remove(name) == null) throw 'no saved layout `$name`';
    return 'removed layout $name';
  }

  @override
  Future<String?> liveLayout() async {
    extraCalls.add('layout-live');
    return liveLayoutJson;
  }

  @override
  Future<String> saveLiveLayout(String json) async {
    extraCalls.add('layout-save-live');
    liveLayoutJson = json;
    return 'saved layout';
  }

  @override
  Future<bool> historyOpen() async => historyBarOpen;

  @override
  Future<void> setHistoryOpen(bool open) async {
    extraCalls.add('history-open:$open');
    historyBarOpen = open;
  }

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
  List<JournalAnnotationInfo> annotationList = [];
  List<JournalImageInfo> imageList = [];
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
  Future<List<JournalAnnotationInfo>> entryAnnotations(String entry) async => [
    for (final a in annotationList)
      if (a.entryId == entry) a,
  ];

  @override
  Future<JournalAnnotationInfo> addEntryNote(
    String entry,
    String body, {
    String? words,
    int? nth,
    int? start,
    int? end,
  }) async {
    extraCalls.add('entry-note:$entry:${words ?? ''}');
    final e = entryList.firstWhere((x) => x.id == entry);
    final n = JournalAnnotationInfo(
      id: 'a${annotationList.length + 1}',
      shortId: 'a${annotationList.length + 1}',
      journalId: e.journalId,
      entryId: entry,
      kind: 'note',
      start: start,
      end: end,
      quote: words,
      body: body,
    );
    annotationList = [...annotationList, n];
    return n;
  }

  @override
  Future<JournalAnnotationInfo> addEntryHighlight(
    String entry, {
    String? words,
    int? nth,
    int? start,
    int? end,
    String? color,
    String? comment,
  }) async {
    extraCalls.add('highlight:$entry:${words ?? ''}');
    final e = entryList.firstWhere((x) => x.id == entry);
    final h = JournalAnnotationInfo(
      id: 'a${annotationList.length + 1}',
      shortId: 'a${annotationList.length + 1}',
      journalId: e.journalId,
      entryId: entry,
      kind: 'highlight',
      start: start,
      end: end,
      quote: words,
      body: comment ?? '',
      color: color ?? 'yellow',
    );
    annotationList = [...annotationList, h];
    return h;
  }

  @override
  Future<JournalAnnotationInfo> updateJournalAnnotation(
    String id,
    String body,
  ) async {
    extraCalls.add('ann-edit:$id');
    annotationList = [
      for (final a in annotationList)
        if (a.id == id)
          JournalAnnotationInfo(
            id: a.id,
            shortId: a.shortId,
            journalId: a.journalId,
            entryId: a.entryId,
            kind: a.kind,
            start: a.start,
            end: a.end,
            quote: a.quote,
            body: body,
            color: a.color,
            detached: a.detached,
          )
        else
          a,
    ];
    return annotationList.firstWhere((a) => a.id == id);
  }

  @override
  Future<String> removeJournalAnnotation(String id) async {
    extraCalls.add('ann-remove:$id');
    final a = annotationList.firstWhere((x) => x.id == id);
    annotationList = [
      for (final x in annotationList)
        if (x.id != id) x,
    ];
    return 'removed ${a.kind}';
  }

  @override
  Future<List<JournalImageInfo>> entryImages(String entry) async => [
    for (final i in imageList)
      if (i.entryId == entry) i,
  ];

  @override
  Future<JournalImageInfo> attachJournalImage(
    String entry,
    String path, {
    String? name,
  }) async {
    extraCalls.add('attach:$entry:$path');
    final n = name ?? path.split('/').last;
    final img = JournalImageInfo(
      id: 'img${imageList.length + 1}',
      shortId: 'img${imageList.length + 1}',
      entryId: entry,
      name: n,
      mime: 'image/png',
      size: 21,
    );
    imageList = [...imageList, img];
    return img;
  }

  @override
  Future<String> saveJournalImage(String id, String out) async {
    extraCalls.add('save-image:$id:$out');
    final img = imageList.firstWhere((x) => x.id == id);
    return 'wrote ${img.name} (${img.size} bytes) to $out';
  }

  @override
  Future<Uint8List> journalImageThumbnail(String id, {int? maxPx}) async {
    extraCalls.add('image-thumb:$id');
    imageList.firstWhere((x) => x.id == id);
    // 1×1 opaque red PNG so widget tests can paint Image.memory.
    return Uint8List.fromList(const [
      0x89,
      0x50,
      0x4E,
      0x47,
      0x0D,
      0x0A,
      0x1A,
      0x0A,
      0x00,
      0x00,
      0x00,
      0x0D,
      0x49,
      0x48,
      0x44,
      0x52,
      0x00,
      0x00,
      0x00,
      0x01,
      0x00,
      0x00,
      0x00,
      0x01,
      0x08,
      0x02,
      0x00,
      0x00,
      0x00,
      0x90,
      0x77,
      0x53,
      0xDE,
      0x00,
      0x00,
      0x00,
      0x0C,
      0x49,
      0x44,
      0x41,
      0x54,
      0x08,
      0xD7,
      0x63,
      0xF8,
      0xCF,
      0xC0,
      0x00,
      0x00,
      0x00,
      0x03,
      0x00,
      0x01,
      0x18,
      0xDD,
      0x8D,
      0xB0,
      0x00,
      0x00,
      0x00,
      0x00,
      0x49,
      0x45,
      0x4E,
      0x44,
      0xAE,
      0x42,
      0x60,
      0x82,
    ]);
  }

  @override
  Future<List<JournalEntryInfo>> copyJournalEntries(
    List<String> entries, {
    String? to,
    String? newName,
    String? project,
  }) async {
    extraCalls.add('copy:${entries.join(',')}:${to ?? ''}:${newName ?? ''}');
    String dest = to ?? '';
    if (newName != null && newName.isNotEmpty) {
      final j = await createJournal(newName, project: project);
      dest = j.id;
    }
    final copied = <JournalEntryInfo>[];
    for (final id in entries) {
      final e = entryList.firstWhere((x) => x.id == id);
      final n = JournalEntryInfo(
        id: 'e${entryList.length + copied.length + 1}',
        shortId: 'e${entryList.length + copied.length + 1}',
        journalId: dest,
        title: e.title,
        heading: e.heading,
        body: e.body,
        format: e.format,
      );
      copied.add(n);
    }
    entryList = [...entryList, ...copied];
    return copied;
  }

  @override
  Future<JournalExportInfo> exportJournal(
    String journal, {
    String format = 'pdf',
    required String out,
    List<String> entries = const [],
  }) async {
    extraCalls.add('journal-export:$journal:$format:$out');
    return JournalExportInfo(
      path: out,
      message: 'exported journal $journal to $out ($format)',
    );
  }

  @override
  Future<JournalInfo> importJournal(
    String path, {
    String? project,
    String? name,
  }) async {
    extraCalls.add('journal-import:$path');
    return createJournal(name ?? 'Imported', project: project);
  }

  @override
  Future<JournalExportInfo> printJournal(
    String journal, {
    List<String> entries = const [],
    String? printer,
    String? out,
    bool saveOnly = true,
  }) async {
    extraCalls.add('journal-print:$journal:${out ?? ''}');
    final path = out ?? '/tmp/$journal.pdf';
    return JournalExportInfo(
      path: path,
      message: 'saved the PDF at $path (not sent to a printer)',
    );
  }

  @override
  Future<List<SearchInfo>> searches() async => List.of(searchList);

  SearchInfo _withSearch(
    SearchInfo s, {
    String? id,
    String? shortId,
    String? name,
    String? target,
    String? query,
    String? describe,
    List<String>? sources,
    List<String>? symbols,
    List<String>? periods,
    List<int>? years,
    int? from,
    int? to,
    bool clearFrom = false,
    bool clearTo = false,
    String? priceMetric,
    double? priceMin,
    double? priceMax,
    String? sort,
    bool? desc,
    String? model,
    List<String>? conditions,
  }) => SearchInfo(
    id: id ?? s.id,
    shortId: shortId ?? s.shortId,
    name: name ?? s.name,
    target: target ?? s.target,
    query: query ?? s.query,
    describe: describe ?? s.describe,
    matches: s.matches,
    forkedFrom: s.forkedFrom,
    sources: sources ?? s.sources,
    symbols: symbols ?? s.symbols,
    periods: periods ?? s.periods,
    years: years ?? s.years,
    from: clearFrom ? from : (from ?? s.from),
    to: clearTo ? to : (to ?? s.to),
    priceMetric: priceMetric ?? s.priceMetric,
    priceMin: priceMin ?? s.priceMin,
    priceMax: priceMax ?? s.priceMax,
    sort: sort ?? s.sort,
    desc: desc ?? s.desc,
    model: model ?? s.model,
    conditions: conditions ?? s.conditions,
  );

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
  }) async {
    extraCalls.add('search-save:$name');
    final s = SearchInfo(
      id: 's${searchList.length + 1}',
      shortId: 's${searchList.length + 1}',
      name: name,
      query: query,
      target: target,
      describe: '$target "$query"',
      sources: sources,
      symbols: symbols,
      periods: periods,
      years: years,
      from: from,
      to: to,
      priceMetric: price ?? 'close',
      priceMin: min,
      priceMax: max,
      sort: sort ?? 'year',
      desc: desc,
      model: model,
      conditions: conditions,
    );
    searchList = [...searchList, s];
    return s;
  }

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
  }) async {
    extraCalls.add('search-edit:$id');
    searchList = [
      for (final s in searchList)
        if (s.id == id)
          _withSearch(
            s,
            name: name ?? s.name,
            query: query,
            target: target,
            describe: '$target "$query"',
            sources: sources,
            symbols: symbols,
            periods: periods,
            years: years,
            from: from,
            to: to,
            clearFrom: true,
            clearTo: true,
            priceMetric: price ?? 'close',
            priceMin: min,
            priceMax: max,
            sort: sort ?? 'year',
            desc: desc,
            model: model,
            conditions: conditions,
          )
        else
          s,
    ];
    return searchList.firstWhere((s) => s.id == id);
  }

  @override
  Future<SearchInfo> renameSearch(String id, String name) async {
    extraCalls.add('search-rename:$id');
    searchList = [
      for (final s in searchList)
        if (s.id == id) _withSearch(s, name: name) else s,
    ];
    return searchList.firstWhere((s) => s.id == id);
  }

  @override
  Future<SearchInfo> forkSearch(String id, {String? name}) async {
    extraCalls.add('search-fork:$id');
    final s = searchList.firstWhere((x) => x.id == id);
    final f = _withSearch(
      s,
      id: 's${searchList.length + 1}',
      shortId: 'sf',
      name: name ?? '${s.name} (fork)',
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
  Future<List<FoundEntryInfo>> searchFoundEntries(
    String query, {
    int? from,
    int? to,
  }) async {
    extraCalls.add('found-entries:$query');
    return List.of(foundEntries);
  }

  @override
  Future<List<FoundRecordInfo>> searchFoundRecords(
    String query, {
    int? from,
    int? to,
  }) async {
    extraCalls.add('found-records:$query');
    return List.of(foundRecords);
  }

  @override
  Future<List<ModelInfo>> models() async => List.of(modelList);

  @override
  Future<ModelInfo> model(String key) async => modelList.firstWhere(
    (m) => m.id == key || m.name == key || m.shortId == key,
  );

  @override
  Future<List<RecordInfo>> modelRecords(
    String model, {
    String? text,
    int? limit,
  }) async {
    extraCalls.add('model-records:$model');
    final q = (text ?? '').toLowerCase();
    var rows = [
      for (final r in recordList)
        if (r.modelId == model) r,
    ];
    if (q.isNotEmpty) {
      rows = [
        for (final r in rows)
          if (r.fields.any((f) => f.value.toLowerCase().contains(q))) r,
      ];
    }
    if (limit != null && rows.length > limit) rows = rows.sublist(0, limit);
    return rows;
  }

  @override
  Future<RecordInfo> modelRecord(String model, String id) async {
    extraCalls.add('model-record:$model:$id');
    return recordList.firstWhere((r) => r.modelId == model && r.id == id);
  }

  ModelInfo _findModel(String key) => modelList.firstWhere(
    (m) => m.id == key || m.name == key || m.shortId == key,
  );

  DesignFieldInfo _parseSpec(String spec, {String? id, String mark = '+'}) {
    final name = spec.split(RegExp(r'[:\s]')).first.trim();
    final ty = spec.contains(':')
        ? spec.split(':')[1].trim().split(RegExp(r'\s')).first
        : 'text';
    return DesignFieldInfo(
      id: id ?? 'f${++_fieldSeq}',
      name: name,
      spec: spec,
      ty: ty,
      mark: mark,
    );
  }

  void _putModel(ModelInfo m) {
    modelList = [
      for (final x in modelList)
        if (x.id != m.id) x,
      m,
    ];
  }

  @override
  Future<List<String>> modelFieldTypes() async => const [
    'text',
    'integer',
    'real',
    'bool',
    'timestamp',
    'json',
    'ref:MODEL',
  ];

  @override
  Future<ModelInfo> modelCreate(
    String name, {
    String description = '',
    String fields = '',
    bool draft = true,
  }) async {
    extraCalls.add('model-create:$name');
    final specs = [
      for (final p in fields.split(';'))
        if (p.trim().isNotEmpty) _parseSpec(p.trim()),
    ];
    final id = 'm${++_modelSeq}';
    final m = ModelInfo(
      id: id,
      shortId: id,
      name: name,
      description: description,
      version: draft ? 0 : 1,
      hasDraft: draft,
      status: draft ? 'draft' : 'v1',
      fields: [for (final f in specs) f.name],
      designFields: draft
          ? specs
          : [for (final f in specs) _parseSpec(f.spec, id: f.id, mark: ' ')],
    );
    _putModel(m);
    if (!draft) {
      modelVersions = [
        ...modelVersions,
        ModelVersionInfo(version: 1, current: true, fieldCount: specs.length),
      ];
    }
    return m;
  }

  @override
  Future<ModelInfo> modelRename(
    String key, {
    String? name,
    String? description,
  }) async {
    extraCalls.add('model-rename:$key');
    final m = _findModel(key);
    final n = ModelInfo(
      id: m.id,
      shortId: m.shortId,
      name: name ?? m.name,
      description: description ?? m.description,
      builtin: m.builtin,
      version: m.version,
      isBars: m.isBars,
      fields: m.fields,
      recordCount: m.recordCount,
      hasDraft: m.hasDraft,
      status: m.status,
      designFields: m.designFields,
    );
    _putModel(n);
    return n;
  }

  @override
  Future<ModelInfo> modelEdit(
    String key, {
    List<String> add = const [],
    List<String> change = const [],
    List<String> drop = const [],
    List<String> rename = const [],
    List<String> moves = const [],
  }) async {
    extraCalls.add('model-edit:$key');
    final m = _findModel(key);
    var fields = [...m.designFields];
    for (final spec in add) {
      fields.add(_parseSpec(spec));
    }
    for (final name in drop) {
      fields = [
        for (final f in fields)
          if (f.name == name)
            DesignFieldInfo(
              id: f.id,
              name: f.name,
              spec: f.spec,
              ty: f.ty,
              mark: '-',
              dropped: true,
            )
          else
            f,
      ];
    }
    for (final spec in change) {
      final parsed = _parseSpec(spec);
      fields = [
        for (final f in fields)
          if (f.name == parsed.name)
            DesignFieldInfo(
              id: f.id,
              name: parsed.name,
              spec: spec,
              ty: parsed.ty,
              mark: '~',
            )
          else
            f,
      ];
    }
    final n = ModelInfo(
      id: m.id,
      shortId: m.shortId,
      name: m.name,
      description: m.description,
      builtin: m.builtin,
      version: m.version,
      isBars: m.isBars,
      fields: [
        for (final f in fields)
          if (!f.dropped) f.name,
      ],
      recordCount: m.recordCount,
      hasDraft: true,
      status: 'draft',
      designFields: fields,
    );
    _putModel(n);
    return n;
  }

  @override
  Future<ModelInfo> modelSetField(
    String key,
    String fieldId,
    String spec,
  ) async {
    extraCalls.add('model-set-field:$key:$fieldId');
    final m = _findModel(key);
    final parsed = _parseSpec(spec, id: fieldId, mark: '~');
    final fields = [
      for (final f in m.designFields)
        if (f.id == fieldId) parsed else f,
    ];
    final n = ModelInfo(
      id: m.id,
      shortId: m.shortId,
      name: m.name,
      description: m.description,
      builtin: m.builtin,
      version: m.version,
      isBars: m.isBars,
      fields: [
        for (final f in fields)
          if (!f.dropped) f.name,
      ],
      recordCount: m.recordCount,
      hasDraft: true,
      status: 'draft',
      designFields: fields,
    );
    _putModel(n);
    return n;
  }

  @override
  Future<ModelPlanInfo?> modelPlan(String key) async {
    extraCalls.add('model-plan:$key');
    final m = _findModel(key);
    if (!m.hasDraft) return null;
    return ModelPlanInfo(
      from: m.version,
      to: m.version + 1,
      lines: [
        for (final f in m.designFields)
          if (f.mark.trim().isNotEmpty) '${f.mark} ${f.name}',
      ],
    );
  }

  @override
  Future<ModelPlanInfo> modelApply(
    String key, {
    bool destructive = false,
  }) async {
    extraCalls.add('model-apply:$key:$destructive');
    final m = _findModel(key);
    final kept = [
      for (final f in m.designFields)
        if (!f.dropped) _parseSpec(f.spec, id: f.id, mark: ' '),
    ];
    final n = ModelInfo(
      id: m.id,
      shortId: m.shortId,
      name: m.name,
      description: m.description,
      builtin: m.builtin,
      version: m.version + 1,
      isBars: m.isBars,
      fields: [for (final f in kept) f.name],
      recordCount: m.recordCount,
      hasDraft: false,
      status: 'v${m.version + 1}',
      designFields: kept,
    );
    _putModel(n);
    modelVersions = [
      for (final v in modelVersions)
        ModelVersionInfo(
          version: v.version,
          createdAt: v.createdAt,
          note: v.note,
          author: v.author,
          fieldCount: v.fieldCount,
          current: false,
          steps: v.steps,
        ),
      ModelVersionInfo(
        version: n.version,
        current: true,
        fieldCount: kept.length,
      ),
    ];
    return ModelPlanInfo(from: m.version, to: n.version, lines: ['applied']);
  }

  @override
  Future<ModelInfo> modelDiscard(String key) async {
    extraCalls.add('model-discard:$key');
    final m = _findModel(key);
    final fields = [
      for (final f in m.designFields)
        if (f.mark != '+')
          DesignFieldInfo(
            id: f.id,
            name: f.name,
            spec: f.spec,
            ty: f.ty,
            mark: ' ',
            dropped: false,
          ),
    ];
    final n = ModelInfo(
      id: m.id,
      shortId: m.shortId,
      name: m.name,
      description: m.description,
      builtin: m.builtin,
      version: m.version,
      isBars: m.isBars,
      fields: [for (final f in fields) f.name],
      recordCount: m.recordCount,
      hasDraft: false,
      status: m.version == 0 ? '' : 'v${m.version}',
      designFields: fields,
    );
    _putModel(n);
    return n;
  }

  @override
  Future<List<ModelVersionInfo>> modelHistory(String key) async {
    extraCalls.add('model-history:$key');
    return List.of(modelVersions);
  }

  @override
  Future<ModelPlanInfo> modelRollbackPlan(String key, int version) async {
    extraCalls.add('model-rollback-plan:$key:$version');
    return ModelPlanInfo(from: _findModel(key).version, to: version + 1);
  }

  @override
  Future<ModelPlanInfo> modelRollback(
    String key,
    int version, {
    bool destructive = false,
  }) async {
    extraCalls.add('model-rollback:$key:$version:$destructive');
    return modelApply(key, destructive: destructive);
  }

  @override
  Future<String> modelRemove(String key, {bool destructive = false}) async {
    extraCalls.add('model-remove:$key:$destructive');
    final m = _findModel(key);
    modelList = [
      for (final x in modelList)
        if (x.id != m.id) x,
    ];
    return 'removed model ${m.name} (0 record(s))';
  }

  @override
  Future<List<String>> modelRuleIds(String key) async {
    extraCalls.add('model-rule-ids:$key');
    final m = _findModel(key);
    return [
      for (final r in ruleList)
        if (r.home == 'model ${m.name}' || r.home == 'model:${m.name}') r.id,
    ];
  }

  ModelInfo _withCount(ModelInfo m, int n) => ModelInfo(
    id: m.id,
    shortId: m.shortId,
    name: m.name,
    description: m.description,
    builtin: m.builtin,
    version: m.version,
    isBars: m.isBars,
    fields: m.fields,
    recordCount: n,
    hasDraft: m.hasDraft,
    status: m.status,
    designFields: m.designFields,
  );

  List<RecordField> _parseValues(String values) {
    final parts = values.contains(';')
        ? values.split(';')
        : values.split(RegExp(r'\s+'));
    return [
      for (final p in parts)
        if (p.contains('='))
          RecordField(
            name: p.split('=').first.trim(),
            value: p.substring(p.indexOf('=') + 1).trim(),
          ),
    ];
  }

  @override
  Future<ModelInfo> modelRestoreField(String key, String fieldId) async {
    extraCalls.add('model-restore:$key:$fieldId');
    final m = _findModel(key);
    final fields = <DesignFieldInfo>[];
    var found = false;
    for (final f in m.designFields) {
      if (f.id != fieldId) {
        fields.add(f);
        continue;
      }
      found = true;
      if (f.mark.trim() == '+') continue;
      fields.add(
        DesignFieldInfo(
          id: f.id,
          name: f.name,
          spec: f.spec,
          ty: f.ty,
          required: f.required,
          unique: f.unique,
          indexed: f.indexed,
          defaultValue: f.defaultValue,
          choices: f.choices,
          about: f.about,
          mark: ' ',
          dropped: false,
          builtin: f.builtin,
        ),
      );
    }
    if (!found) throw StateError('no field $fieldId');
    final n = ModelInfo(
      id: m.id,
      shortId: m.shortId,
      name: m.name,
      description: m.description,
      builtin: m.builtin,
      version: m.version,
      isBars: m.isBars,
      fields: [
        for (final f in fields)
          if (!f.dropped) f.name,
      ],
      recordCount: m.recordCount,
      hasDraft: true,
      status: 'draft',
      designFields: fields,
    );
    _putModel(n);
    return n;
  }

  @override
  Future<List<ModelTemplateInfo>> modelTemplates() async {
    extraCalls.add('model-templates');
    return const [
      ModelTemplateInfo(
        name: 'bars',
        source: 'builtin:bars',
        description: 'OHLCV bars',
        fieldCount: 8,
      ),
    ];
  }

  @override
  Future<String> modelExportTemplate(
    String key,
    String path, {
    bool records = false,
  }) async {
    extraCalls.add('model-export:$key:$path:$records');
    final m = _findModel(key);
    return 'wrote template ${m.name} to $path';
  }

  @override
  Future<ModelImportPlanInfo> modelImportPreview(
    String source, {
    String? name,
    bool merge = false,
    bool draft = false,
    bool noRecords = false,
  }) async {
    extraCalls.add('model-import-preview:$source');
    final want = (name == null || name.isEmpty) ? 'imported' : name;
    return ModelImportPlanInfo(
      templateName: source.startsWith('builtin:')
          ? source.substring(8)
          : 'file',
      model: want,
      merge: merge,
      summary: 'preview $source as $want',
    );
  }

  @override
  Future<String> modelImportTemplate(
    String source, {
    String? name,
    bool merge = false,
    bool draft = false,
    bool noRecords = false,
    bool destructive = false,
  }) async {
    extraCalls.add('model-import:$source:${name ?? ""}:$merge');
    if (!merge) {
      await modelCreate(name ?? 'imported', fields: 'memo:text', draft: draft);
    }
    return 'imported $source as ${name ?? "imported"}';
  }

  @override
  Future<RecordInfo> modelRecordCreate(String model, String values) async {
    extraCalls.add('model-record-create:$model:$values');
    final m = _findModel(model);
    final id = 'r${++_recordSeq}';
    final rec = RecordInfo(
      id: id,
      shortId: id,
      modelId: m.id,
      fields: _parseValues(values),
    );
    recordList = [...recordList, rec];
    _putModel(_withCount(m, m.recordCount + 1));
    return rec;
  }

  @override
  Future<RecordInfo> modelRecordUpdate(
    String model,
    String id,
    String values,
  ) async {
    extraCalls.add('model-record-update:$model:$id:$values');
    final old = recordList.firstWhere((r) => r.id == id || r.shortId == id);
    final next = Map<String, String>.from({
      for (final f in old.fields) f.name: f.value,
    });
    for (final f in _parseValues(values)) {
      next[f.name] = f.value;
    }
    final rec = RecordInfo(
      id: old.id,
      shortId: old.shortId,
      modelId: old.modelId,
      version: old.version + 1,
      fields: [
        for (final e in next.entries) RecordField(name: e.key, value: e.value),
      ],
    );
    recordList = [
      for (final r in recordList)
        if (r.id == old.id) rec else r,
    ];
    return rec;
  }

  @override
  Future<String> modelRecordUpdateMany(
    String model,
    String text,
    String values,
  ) async {
    extraCalls.add('model-record-update-many:$model:$text:$values');
    final rows = await modelRecords(model, text: text);
    for (final r in rows) {
      await modelRecordUpdate(model, r.id, values);
    }
    return 'updated ${rows.length} records';
  }

  @override
  Future<String> modelRecordDelete(String model, String id) async {
    extraCalls.add('model-record-delete:$model:$id');
    final m = _findModel(model);
    recordList = [
      for (final r in recordList)
        if (!(r.modelId == m.id && (r.id == id || r.shortId == id))) r,
    ];
    _putModel(_withCount(m, m.recordCount > 0 ? m.recordCount - 1 : 0));
    return 'deleted record $id';
  }

  @override
  Future<List<String>> modelRuleTry(String rule) async {
    extraCalls.add('model-rule-try:$rule');
    return ['ran $rule on #1 import: ok', '  hi'];
  }

  @override
  Future<List<HistoryRowInfo>> historyTree() async => List.of(historyList);

  HistoryMoveInfo _historyMove() =>
      const HistoryMoveInfo(theme: 'tokyo-night', mode: 'dark');

  @override
  Future<HistoryMoveInfo> historyBack() async {
    extraCalls.add('history-back');
    return _historyMove();
  }

  @override
  Future<HistoryMoveInfo> historyForward() async {
    extraCalls.add('history-forward');
    return _historyMove();
  }

  @override
  Future<HistoryMoveInfo> historyGoto(int? id) async {
    extraCalls.add('history-goto:$id');
    return _historyMove();
  }

  @override
  Future<List<AuthorInfo>> authors() async => List.of(authorList);

  @override
  Future<AuthorInfo> addAuthor(
    String name, {
    String? email,
    List<String> details = const [],
  }) async {
    extraCalls.add('author-add:$name');
    final a = AuthorInfo(
      id: 'a${authorList.length + 1}',
      shortId: 'a${authorList.length + 1}',
      name: name,
      email: email ?? '',
      details: details,
      current: authorList.isEmpty,
    );
    authorList = [...authorList, a];
    return a;
  }

  @override
  Future<AuthorInfo> updateAuthor(
    String id, {
    String? name,
    String? email,
    List<String> details = const [],
    bool replaceDetails = false,
  }) async {
    extraCalls.add('author-edit:$id');
    AuthorInfo? updated;
    final next = <AuthorInfo>[];
    for (final a in authorList) {
      if (a.id == id) {
        updated = AuthorInfo(
          id: a.id,
          shortId: a.shortId,
          name: name ?? a.name,
          email: email ?? a.email,
          details: replaceDetails
              ? details
              : (details.isEmpty ? a.details : details),
          current: a.current,
        );
        next.add(updated);
      } else {
        next.add(a);
      }
    }
    authorList = next;
    if (updated == null) throw 'no author `$id`';
    return updated;
  }

  @override
  Future<String> removeAuthor(String id) async {
    extraCalls.add('author-remove:$id');
    authorList = [
      for (final a in authorList)
        if (a.id != id) a,
    ];
    return 'removed $id';
  }

  @override
  Future<String> setCurrentAuthor(String? id) async {
    extraCalls.add('author-current:$id');
    authorList = [
      for (final a in authorList)
        AuthorInfo(
          id: a.id,
          shortId: a.shortId,
          name: a.name,
          email: a.email,
          details: a.details,
          current: a.id == id,
        ),
    ];
    return id == null ? 'no current author' : '$id is current';
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
    int? from,
    int? to,
    int? rows,
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
  Future<ReportExportInfo> exportReport(
    String report, {
    String format = 'pdf',
    required String out,
  }) async {
    extraCalls.add('report-export:$report:$format:$out');
    return ReportExportInfo(
      path: out,
      message: 'exported report $report to $out ($format)',
    );
  }

  @override
  Future<String> previewReport(String report, {int width = 100}) async {
    extraCalls.add('report-preview:$report');
    final r = reportList.firstWhere((x) => x.id == report);
    return '${r.name}\n${r.description}'.trim();
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

  List<CodeSpaceInfo> codeList = [];
  List<LivePipeInfo> pipeList = [];
  List<PipelineInfo> pipelineList = [];
  List<CodeParserInfo> parserList = const [
    CodeParserInfo(
      name: 'parse-csv',
      about: 'CSV as it arrived',
      installed: false,
    ),
    CodeParserInfo(
      name: 'to-bars',
      about: 'rows to OHLCV bars',
      installed: false,
    ),
  ];
  int _codeN = 0;
  int _pipeN = 0;

  @override
  Future<List<CodeSpaceInfo>> codeSpaces() async => List.of(codeList);

  @override
  Future<CodeSpaceInfo> codeCreate(
    String name, {
    String lang = 'math',
    String description = '',
    String? source,
    String? from,
    String? input,
  }) async {
    extraCalls.add('code-create:$name:$lang');
    _codeN += 1;
    final c = CodeSpaceInfo(
      id: 'c$_codeN',
      shortId: 'c$_codeN',
      name: name,
      description: description,
      lang: lang,
      source:
          source ?? (from != null ? 'from $from' : 'sma20 = sma(close, 20)'),
      input: input ?? 'none',
    );
    codeList = [...codeList, c];
    return c;
  }

  @override
  Future<CodeSpaceInfo> codeEdit(
    String space, {
    String? name,
    String? description,
    String? lang,
    String? source,
    String? input,
  }) async {
    extraCalls.add('code-edit:$space');
    codeList = [
      for (final c in codeList)
        if (c.id == space || c.shortId == space || c.name == space)
          CodeSpaceInfo(
            id: c.id,
            shortId: c.shortId,
            name: name ?? c.name,
            description: description ?? c.description,
            lang: lang ?? c.lang,
            source: source ?? c.source,
            input: input ?? c.input,
            scope: c.scope,
            approved: false,
            trustHash: c.trustHash,
            lastStatus: c.lastStatus,
            lastError: c.lastError,
            lastAt: c.lastAt,
            lastMs: c.lastMs,
            lastWrites: c.lastWrites,
          )
        else
          c,
    ];
    return codeList.firstWhere((c) => c.id == space || c.name == space);
  }

  @override
  Future<String> codeRemove(String space) async {
    extraCalls.add('code-remove:$space');
    final n = codeList.firstWhere((c) => c.id == space || c.name == space).name;
    codeList = [
      for (final c in codeList)
        if (c.id != space && c.name != space) c,
    ];
    return 'removed code space $n';
  }

  @override
  Future<String> codeApprove(String space) async {
    extraCalls.add('code-approve:$space');
    codeList = [
      for (final c in codeList)
        if (c.id == space || c.name == space)
          CodeSpaceInfo(
            id: c.id,
            shortId: c.shortId,
            name: c.name,
            description: c.description,
            lang: c.lang,
            source: c.source,
            input: c.input,
            scope: c.scope,
            approved: true,
            trustHash: 'abcd1234abcd1234',
            lastStatus: c.lastStatus,
            lastError: c.lastError,
            lastAt: c.lastAt,
            lastMs: c.lastMs,
            lastWrites: c.lastWrites,
          )
        else
          c,
    ];
    return 'approved code space $space';
  }

  @override
  Future<List<String>> codeCheck(String space) async {
    extraCalls.add('code-check:$space');
    return ['ok', 'approval: not approved'];
  }

  @override
  Future<CodeRunResult> codeBuild(String space) async {
    extraCalls.add('code-build:$space');
    return const CodeRunResult(status: 'built', lines: ['built']);
  }

  @override
  Future<CodeRunResult> codeRun(String space) async {
    extraCalls.add('code-run:$space');
    codeList = [
      for (final c in codeList)
        if (c.id == space || c.name == space)
          CodeSpaceInfo(
            id: c.id,
            shortId: c.shortId,
            name: c.name,
            description: c.description,
            lang: c.lang,
            source: c.source,
            input: c.input,
            scope: c.scope,
            approved: true,
            trustHash: c.trustHash,
            lastStatus: 'ok',
            lastError: '',
            lastAt: 1,
            lastMs: 4,
            lastWrites: 0,
          )
        else
          c,
    ];
    final name = codeList
        .firstWhere((c) => c.id == space || c.name == space)
        .name;
    pipeFrames['code:$name'] = PipeFrameInfo(
      key: 'code:$name',
      columns: const ['value'],
      cells: const ['101'],
      rowCount: 1,
    );
    return const CodeRunResult(status: 'ok', lines: ['mean close 101'], ms: 4);
  }

  @override
  Future<PipeFrameInfo?> latestPipeFrame(String key) async {
    extraCalls.add('pipe-frame:$key');
    return pipeFrames[key];
  }

  @override
  Future<List<CodeParserInfo>> codeParsers() async => List.of(parserList);

  @override
  Future<String> codeParsersInstall({String? name}) async {
    extraCalls.add('code-parsers-install:${name ?? "all"}');
    parserList = [
      for (final p in parserList)
        if (name == null || name.isEmpty || p.name == name)
          CodeParserInfo(name: p.name, about: p.about, installed: true)
        else
          p,
    ];
    return 'installed';
  }

  @override
  Future<List<LivePipeInfo>> pipesLive() async => List.of(pipeList);

  @override
  Future<PipeRunResult> pipeRun(
    String source, {
    List<String> steps = const [],
    List<String> to = const [],
    String? stdin,
    String? stdinFile,
    String? format,
    int chunk = 0,
    bool keepHeader = false,
  }) async {
    extraCalls.add('pipe-run:$source:${steps.join("|")}');
    if (source == '-' ||
        stdin != null ||
        stdinFile != null ||
        format != null ||
        chunk > 0 ||
        keepHeader) {
      extraCalls.add(
        'pipe-stdin:$source:format=${format ?? ''}:chunk=$chunk:file=${stdinFile ?? ''}:header=$keepHeader:text=${stdin ?? ''}',
      );
    }
    _pipeN += 1;
    pipeList = [
      LivePipeInfo(
        token: 't$_pipeN',
        origin: 'app',
        status: 'done',
        hops: [
          LiveHopInfo(name: source, status: 'ok', rows: 3),
          for (final s in steps) LiveHopInfo(name: s, status: 'ok', rows: 3),
        ],
        saved: to,
        updated: 1,
      ),
      ...pipeList,
    ];
    return PipeRunResult(
      hops: [source, ...steps],
      saved: to,
      lines: ['pipe done'],
    );
  }

  @override
  Future<List<PipelineInfo>> pipelines() async => List.of(pipelineList);

  @override
  Future<PipelineInfo> pipelineCreate(
    String name, {
    String description = '',
    List<String> nodes = const [],
    List<String> edges = const [],
  }) async {
    extraCalls.add('pipeline-create:$name');
    _codeN += 1;
    final p = PipelineInfo(
      id: 'p$_codeN',
      shortId: 'p$_codeN',
      name: name,
      description: description,
      nodes: nodes.length,
      nodeSpecs: nodes,
      edgeSpecs: edges,
      graphLines: [...nodes, ...edges],
      sinks: nodes.where((n) => n.contains('model:')).join(', '),
    );
    pipelineList = [...pipelineList, p];
    return p;
  }

  @override
  Future<PipelineInfo> pipelineEdit(
    String pipeline, {
    String? name,
    String? description,
    List<String> nodes = const [],
    List<String> edges = const [],
    List<String> removeNodes = const [],
    List<String> removeEdges = const [],
  }) async {
    extraCalls.add('pipeline-edit:$pipeline');
    pipelineList = [
      for (final p in pipelineList)
        if (p.id == pipeline || p.name == pipeline)
          PipelineInfo(
            id: p.id,
            shortId: p.shortId,
            name: name ?? p.name,
            description: description ?? p.description,
            version: p.version + 1,
            draft: p.draft,
            nodes: nodes.isEmpty ? p.nodes : nodes.length,
            sinks: p.sinks,
            problems: p.problems,
            nodeSpecs: nodes.isEmpty ? p.nodeSpecs : nodes,
            edgeSpecs: edges.isEmpty ? p.edgeSpecs : edges,
            graphLines: [
              ...(nodes.isEmpty ? p.nodeSpecs : nodes),
              ...(edges.isEmpty ? p.edgeSpecs : edges),
            ],
          )
        else
          p,
    ];
    return pipelineList.firstWhere(
      (p) => p.id == pipeline || p.name == pipeline,
    );
  }

  @override
  Future<String> pipelineRemove(String pipeline) async {
    extraCalls.add('pipeline-remove:$pipeline');
    final n = pipelineList
        .firstWhere((p) => p.id == pipeline || p.name == pipeline)
        .name;
    pipelineList = [
      for (final p in pipelineList)
        if (p.id != pipeline && p.name != pipeline) p,
    ];
    return 'removed pipeline $n';
  }

  @override
  Future<String> pipelineApprove(String pipeline) async {
    extraCalls.add('pipeline-approve:$pipeline');
    return 'approved $pipeline';
  }

  @override
  Future<PipelineRunResult> pipelineRun(
    String pipeline, {
    String? stdin,
    String? stdinFile,
    int chunk = 0,
    bool keepHeader = false,
  }) async {
    extraCalls.add('pipeline-run:$pipeline');
    if (stdin != null || stdinFile != null || chunk > 0 || keepHeader) {
      extraCalls.add(
        'pipeline-run-stdin:$pipeline:chunk=$chunk:file=${stdinFile ?? ''}:header=$keepHeader',
      );
    }
    return const PipelineRunResult(
      ok: true,
      lines: ['src  ok'],
      landed: ['3 rows → model bars'],
    );
  }

  @override
  Future<PipelineInfo> pipelineTee(
    String pipeline,
    String node,
    String sink,
  ) async {
    extraCalls.add('pipeline-tee:$pipeline:$node:$sink');
    pipelineList = [
      for (final p in pipelineList)
        if (p.id == pipeline || p.name == pipeline)
          PipelineInfo(
            id: p.id,
            shortId: p.shortId,
            name: p.name,
            description: p.description,
            version: p.version + 1,
            draft: p.draft,
            nodes: p.nodes,
            sinks: p.sinks,
            problems: p.problems,
            nodeSpecs: [
              for (final n in p.nodeSpecs)
                n.split('=').first == node ? '$n+$sink' : n,
            ],
            edgeSpecs: p.edgeSpecs,
            graphLines: [...p.graphLines, '$node + $sink'],
          )
        else
          p,
    ];
    return pipelineList.firstWhere(
      (p) => p.id == pipeline || p.name == pipeline,
      orElse: () => pipelineList.first,
    );
  }

  @override
  Future<PipelineInfo> pipelineEndInModel(
    String pipeline,
    String node,
    String model,
  ) async {
    extraCalls.add('pipeline-end-in-model:$pipeline:$node:$model');
    final id = '${node}_out';
    pipelineList = [
      for (final p in pipelineList)
        if (p.id == pipeline || p.name == pipeline)
          PipelineInfo(
            id: p.id,
            shortId: p.shortId,
            name: p.name,
            description: p.description,
            version: p.version + 1,
            draft: p.draft,
            nodes: p.nodes + 1,
            sinks: model,
            problems: p.problems,
            nodeSpecs: [...p.nodeSpecs, '$id=model:$model'],
            edgeSpecs: [...p.edgeSpecs, '$node>$id'],
            graphLines: [...p.graphLines, '$id = model:$model'],
          )
        else
          p,
    ];
    return pipelineList.firstWhere(
      (p) => p.id == pipeline || p.name == pipeline,
      orElse: () => pipelineList.first,
    );
  }

  @override
  Future<PipelineMonitor> pipelineMonitor(String pipeline) async {
    extraCalls.add('pipeline-monitor:$pipeline');
    final p = pipelineList.cast<PipelineInfo?>().firstWhere(
      (x) => x!.id == pipeline || x.name == pipeline,
      orElse: () => null,
    );
    return PipelineMonitor(
      pipeline: p?.name ?? pipeline,
      state: 'not run yet',
      nodes: [
        for (final spec in p?.nodeSpecs ?? const <String>[])
          PipelineMonitorNode(
            id: spec.split('=').first,
            kind: spec.contains('=') ? spec.split('=').last : spec,
            status: 'idle',
          ),
      ],
      landed: const [],
    );
  }

  List<McpServerInfo> mcpServerList = const [];
  List<McpClientInfo> mcpClientList = const [];
  List<McpToolInfo> mcpToolList = const [];
  List<PeerMessageInfo> peerMessageList = const [];
  List<LiveSessionInfo> liveSessionList = const [];

  @override
  Future<List<McpServerInfo>> mcpServers() async => mcpServerList;

  @override
  Future<List<McpClientInfo>> mcpClients() async => mcpClientList;

  @override
  Future<List<McpToolInfo>> mcpServerTools(String server) async {
    extraCalls.add('mcp-tools:$server');
    return mcpToolList;
  }

  @override
  Future<String> mcpApproveTool(
    String server,
    String tool, {
    bool on = true,
  }) async {
    extraCalls.add('mcp-approve:$server:$tool:$on');
    mcpToolList = [
      for (final t in mcpToolList)
        if (t.name == tool)
          McpToolInfo(name: t.name, description: t.description, approved: on)
        else
          t,
    ];
    return on ? 'approved $server/$tool' : '$server/$tool needs approval again';
  }

  @override
  Future<String> mcpRemoveServer(String server) async {
    extraCalls.add('mcp-remove-server:$server');
    mcpServerList = [
      for (final s in mcpServerList)
        if (s.id != server && s.name != server) s,
    ];
    return 'removed $server';
  }

  @override
  Future<String> mcpClientRole(String client, String role) async {
    extraCalls.add('mcp-client-role:$client:$role');
    mcpClientList = [
      for (final c in mcpClientList)
        if (c.id == client || c.name == client)
          McpClientInfo(
            id: c.id,
            name: c.name,
            role: role,
            createdAt: c.createdAt,
            lastSeenAt: c.lastSeenAt,
            revoked: c.revoked,
          )
        else
          c,
    ];
    return '$client is now $role';
  }

  @override
  Future<String> mcpClientRevoke(String client, {bool revoke = true}) async {
    extraCalls.add('mcp-client-revoke:$client:$revoke');
    return '${revoke ? "revoked" : "restored"} $client';
  }

  @override
  Future<String> mcpRemoveClient(String client) async {
    extraCalls.add('mcp-remove-client:$client');
    mcpClientList = [
      for (final c in mcpClientList)
        if (c.id != client && c.name != client) c,
    ];
    return 'removed $client';
  }

  @override
  Future<List<PeerMessageInfo>> peerMessages({String? peer}) async => [
    for (final m in peerMessageList)
      if (peer == null || peer.isEmpty || m.peer == peer || m.peerLabel == peer)
        m,
  ];

  @override
  Future<String> peerMessagesMarkRead({String? peer}) async {
    extraCalls.add('peer-messages-read:${peer ?? ""}');
    peerMessageList = [
      for (final m in peerMessageList)
        if (peer == null ||
            peer.isEmpty ||
            m.peer == peer ||
            m.peerLabel == peer)
          PeerMessageInfo(
            id: m.id,
            peer: m.peer,
            peerLabel: m.peerLabel,
            direction: m.direction,
            about: m.about,
            text: m.text,
            createdAt: m.createdAt,
            delivered: m.delivered,
            read: true,
            error: m.error,
          )
        else
          m,
    ];
    return 'marked read';
  }

  @override
  Future<List<LiveSessionInfo>> liveSessions() async => liveSessionList;

  @override
  Future<String> pipelineExport(
    String pipeline,
    String path, {
    bool templates = true,
  }) async {
    extraCalls.add('pipeline-export:$pipeline:$path');
    return 'wrote $path';
  }

  @override
  Future<List<String>> pipelineImportPlan(
    String path, {
    String? name,
    bool merge = false,
  }) async {
    extraCalls.add('pipeline-import-plan:$path');
    return ['pipeline demo: new'];
  }

  @override
  Future<String> pipelineImport(
    String path, {
    String? name,
    bool merge = false,
  }) async {
    extraCalls.add('pipeline-import:$path');
    return 'imported pipeline demo v1';
  }

  List<RuleInfo> ruleList = [];
  List<ReviewInfo> reviewList = [];
  List<ReviewingInfo> reviewingList = [];
  List<NotificationInfo> notificationList = [];
  List<NotificationKindInfo> notificationKindList = const [
    NotificationKindInfo(kind: 'review.comment', about: 'a reviewer commented'),
    NotificationKindInfo(kind: 'notify', about: 'a warning from Muninn'),
  ];
  final Map<String, List<ReviewItemInfo>> reviewItems = {};
  int _ruleN = 0;
  int _reviewN = 0;
  int _noteN = 0;

  @override
  Future<List<RuleInfo>> rules() async => List.of(ruleList);

  @override
  Future<RuleInfo> ruleCreate(
    String name, {
    String description = '',
    String on = '',
    String when = '',
    String then = '',
    String home = '',
    bool related = false,
  }) async {
    extraCalls.add('rule-create:$name:$on');
    _ruleN += 1;
    final r = RuleInfo(
      id: 'r$_ruleN',
      shortId: 'r$_ruleN',
      name: name,
      description: description,
      onText: on,
      whenText: when,
      thenText: then,
      home: home.isEmpty
          ? 'standalone'
          : home.contains(':')
          ? home.replaceFirst(':', ' ')
          : home,
      brief: '$on → $then',
      related: related,
      enabled: true,
    );
    ruleList = [...ruleList, r];
    return r;
  }

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
  }) async {
    extraCalls.add('rule-edit:$rule');
    ruleList = [
      for (final r in ruleList)
        if (r.id == rule || r.name == rule)
          RuleInfo(
            id: r.id,
            shortId: r.shortId,
            name: name ?? r.name,
            description: description ?? r.description,
            enabled: r.enabled,
            home: home ?? r.home,
            brief: r.brief,
            onText: on ?? r.onText,
            whenText: when ?? r.whenText,
            thenText: then ?? r.thenText,
            related: related ?? r.related,
            runs: r.runs,
            lastStatus: r.lastStatus,
          )
        else
          r,
    ];
    return ruleList.firstWhere((r) => r.id == rule || r.name == rule);
  }

  @override
  Future<String> ruleRemove(String rule) async {
    extraCalls.add('rule-remove:$rule');
    final n = ruleList.firstWhere((r) => r.id == rule || r.name == rule).name;
    ruleList = [
      for (final r in ruleList)
        if (r.id != rule && r.name != rule) r,
    ];
    return 'removed rule $n';
  }

  @override
  Future<String> ruleEnable(String rule) async {
    extraCalls.add('rule-enable:$rule');
    ruleList = [
      for (final r in ruleList)
        if (r.id == rule || r.name == rule)
          RuleInfo(
            id: r.id,
            shortId: r.shortId,
            name: r.name,
            description: r.description,
            enabled: true,
            home: r.home,
            brief: r.brief,
            onText: r.onText,
            whenText: r.whenText,
            thenText: r.thenText,
            related: r.related,
            runs: r.runs,
          )
        else
          r,
    ];
    return 'rule $rule is on';
  }

  @override
  Future<String> ruleDisable(String rule) async {
    extraCalls.add('rule-disable:$rule');
    ruleList = [
      for (final r in ruleList)
        if (r.id == rule || r.name == rule)
          RuleInfo(
            id: r.id,
            shortId: r.shortId,
            name: r.name,
            description: r.description,
            enabled: false,
            home: r.home,
            brief: r.brief,
            onText: r.onText,
            whenText: r.whenText,
            thenText: r.thenText,
            related: r.related,
            runs: r.runs,
          )
        else
          r,
    ];
    return 'rule $rule is off';
  }

  @override
  Future<List<String>> ruleShow(String rule) async => [
    'name: $rule',
    'state: on',
  ];

  @override
  Future<List<String>> ruleTest({String? rule, int? action}) async {
    extraCalls.add('rule-test:${rule ?? "all"}');
    return ['rule ${rule ?? "all"} vs #1 import:', '  → matches'];
  }

  @override
  Future<List<String>> ruleDryRun({String? rule, int? action}) async {
    extraCalls.add('rule-dry-run:${rule ?? "all"}');
    return ['rule ${rule ?? "all"} vs #1 import:', '  would show "hi"'];
  }

  @override
  Future<String> ruleTick() async {
    extraCalls.add('rule-tick');
    return 'no scheduled rules are due';
  }

  @override
  Future<List<RuleEventInfo>> ruleEvents({String? kind}) async => [
    const RuleEventInfo(
      kind: 'import',
      about: 'data was imported',
      fields: 'inserted, path',
    ),
  ];

  @override
  Future<List<NotificationInfo>> notifications({
    bool unread = false,
    int limit = 50,
  }) async {
    final all = unread
        ? notificationList.where((n) => !n.read)
        : notificationList;
    return all.take(limit).toList();
  }

  @override
  Future<int> notificationsUnreadCount() async =>
      notificationList.where((n) => !n.read).length;

  NotificationInfo _mark(NotificationInfo n, bool read) => NotificationInfo(
    id: n.id,
    shortId: n.shortId,
    title: n.title,
    kind: n.kind,
    level: n.level,
    body: n.body,
    link: n.link,
    linkLabel: n.linkLabel,
    createdAt: n.createdAt,
    read: read,
    readAt: read ? 1 : 0,
  );

  @override
  Future<NotificationInfo> notificationShow(String id) async {
    extraCalls.add('notify-show:$id');
    notificationList = [
      for (final n in notificationList)
        if (n.id == id || n.shortId == id) _mark(n, true) else n,
    ];
    return notificationList.firstWhere((n) => n.id == id || n.shortId == id);
  }

  @override
  Future<String> notificationRead(String id) async {
    extraCalls.add('notify-read:$id');
    notificationList = [
      for (final n in notificationList)
        if (n.id == id || n.shortId == id) _mark(n, true) else n,
    ];
    return 'read: $id';
  }

  @override
  Future<String> notificationUnread(String id) async {
    extraCalls.add('notify-unread:$id');
    notificationList = [
      for (final n in notificationList)
        if (n.id == id || n.shortId == id) _mark(n, false) else n,
    ];
    return 'unread: $id';
  }

  @override
  Future<String> notificationReadAll() async {
    extraCalls.add('notify-read-all');
    notificationList = [for (final n in notificationList) _mark(n, true)];
    return 'marked ${notificationList.length} read';
  }

  @override
  Future<NotificationInfo> notificationSend(
    String title, {
    String body = '',
    String? link,
    String level = 'info',
  }) async {
    extraCalls.add('notify-send:$title');
    _noteN += 1;
    final n = NotificationInfo(
      id: 'n$_noteN',
      shortId: 'n$_noteN',
      title: title,
      body: body,
      link: link ?? '',
      level: level,
      kind: 'notify',
    );
    notificationList = [n, ...notificationList];
    return n;
  }

  @override
  Future<List<NotificationKindInfo>> notificationKinds() async =>
      List.of(notificationKindList);

  @override
  Future<String> notificationMute(String kind) async {
    extraCalls.add('notify-mute:$kind');
    notificationKindList = [
      for (final k in notificationKindList)
        if (k.kind == kind)
          NotificationKindInfo(kind: k.kind, about: k.about, muted: true)
        else
          k,
    ];
    return 'muted $kind';
  }

  @override
  Future<String> notificationUnmute(String kind) async {
    extraCalls.add('notify-unmute:$kind');
    notificationKindList = [
      for (final k in notificationKindList)
        if (k.kind == kind)
          NotificationKindInfo(kind: k.kind, about: k.about, muted: false)
        else
          k,
    ];
    return 'unmuted $kind';
  }

  @override
  Future<List<ReviewInfo>> reviews() async => List.of(reviewList);

  @override
  Future<List<ReviewingInfo>> reviewsJoined() async => List.of(reviewingList);

  @override
  Future<ReviewInfo> reviewRequest(
    String kind,
    String target, {
    String title = '',
    String note = '',
    List<String> allow = const [],
  }) async {
    extraCalls.add('review-request:$kind:$target');
    _reviewN += 1;
    final r = ReviewInfo(
      id: 'v$_reviewN',
      shortId: 'v$_reviewN',
      title: title.isEmpty ? '$kind $target' : title,
      kind: kind,
      name: target,
      note: note,
      allow: allow.isEmpty
          ? 'view, comment, suggest, highlight, vote'
          : allow.join(', '),
    );
    reviewList = [...reviewList, r];
    reviewItems[r.id] = [];
    return r;
  }

  @override
  Future<ReviewDetail> reviewShow(String review) async {
    final r = reviewList.firstWhere(
      (x) => x.id == review || x.name == review || x.shortId == review,
    );
    return ReviewDetail(
      review: r,
      items: List.of(reviewItems[r.id] ?? const []),
      youMay: r.allow,
      note: r.note,
    );
  }

  @override
  Future<List<String>> reviewView(String review) async => ['name: $review'];

  @override
  Future<String> reviewInvite(
    String review, {
    String label = '',
    String? peer,
  }) async {
    extraCalls.add('review-invite:$review');
    return 'mnnn-x1-demo-link';
  }

  @override
  Future<String> reviewJoin(String token, {String label = ''}) async {
    extraCalls.add('review-join:$token');
    reviewingList = [
      ...reviewingList,
      ReviewingInfo(
        id: 'j${reviewingList.length + 1}',
        shortId: 'j1',
        title: label.isEmpty ? token : label,
        status: 'joined',
      ),
    ];
    return 'reviewing $token';
  }

  @override
  Future<String> reviewSync(String review) async {
    extraCalls.add('review-sync:$review');
    return 'synced $review';
  }

  @override
  Future<String> reviewComment(
    String review,
    String text, {
    String? field,
    String? at,
    int nth = 1,
    String? entry,
    String? reply,
  }) async {
    extraCalls.add('review-comment:$review');
    final r = reviewList.firstWhere(
      (x) => x.id == review || x.name == review || x.shortId == review,
    );
    final items = [...?reviewItems[r.id]];
    items.add(
      ReviewItemInfo(
        id: 'i${items.length + 1}',
        shortId: 'i${items.length + 1}',
        line: 'comment: $text',
        kind: 'comment',
        body: text,
        who: 'author',
        state: 'open',
      ),
    );
    reviewItems[r.id] = items;
    return 'comment: $text';
  }

  @override
  Future<String> reviewSuggest(
    String review,
    String to, {
    String why = '',
    String? field,
    String? at,
    int nth = 1,
    String? entry,
  }) async {
    extraCalls.add('review-suggest:$review');
    return 'suggest → $to';
  }

  @override
  Future<String> reviewHighlight(
    String review, {
    String? field,
    String? at,
    int nth = 1,
    String? entry,
    String? color,
    String note = '',
  }) async {
    extraCalls.add('review-highlight:$review');
    return 'highlight';
  }

  @override
  Future<String> reviewVote(
    String review,
    String vote, {
    String? on,
    String note = '',
  }) async {
    extraCalls.add('review-vote:$review:$vote');
    return 'vote $vote';
  }

  @override
  Future<String> reviewWithdraw(String review, String item) async {
    extraCalls.add('review-withdraw:$review:$item');
    return 'withdrew $item';
  }

  @override
  Future<String> reviewAccept(
    String review, {
    List<String> items = const [],
    bool all = false,
  }) async {
    extraCalls.add('review-accept:$review');
    return 'accepted';
  }

  @override
  Future<String> reviewReject(
    String review, {
    List<String> items = const [],
    bool all = false,
  }) async {
    extraCalls.add('review-reject:$review');
    return 'rejected';
  }

  @override
  Future<String> reviewMerge(String review, {bool force = false}) async {
    extraCalls.add('review-merge:$review');
    return 'merged';
  }

  @override
  Future<String> reviewClose(String review) async {
    extraCalls.add('review-close:$review');
    return 'closed $review';
  }

  @override
  Future<String> reviewReopen(String review) async {
    extraCalls.add('review-reopen:$review');
    return 'opened $review';
  }

  @override
  Future<String> reviewDrop(String review, String reviewer) async {
    extraCalls.add('review-drop:$review:$reviewer');
    return 'took $reviewer off';
  }

  @override
  Future<String> reviewLeave(String review) async {
    extraCalls.add('review-leave:$review');
    return 'left $review';
  }

  @override
  Future<String> reviewRemove(String review) async {
    extraCalls.add('review-remove:$review');
    reviewList = [
      for (final r in reviewList)
        if (r.id != review && r.name != review) r,
    ];
    return 'removed $review';
  }

  @override
  Future<List<String>> reviewRevisions(String kind, String target) async => [
    'no revisions yet',
  ];

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
  Future<List<SourceInfo>> listSources() async =>
      List<SourceInfo>.from(sources);

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
  Future<List<SymbolInfo>> listSymbols() async =>
      List<SymbolInfo>.from(symbols);

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

  NetConfig netCfg = const NetConfig();
  NetConnections connections = const NetConnections(
    me: NetMeInfo(userId: 'me', shortId: 'meuser', name: 'alice', nodes: 1),
    connections: [
      NetConnectionInfo(
        userId: 'u2',
        shortId: 'bobuser',
        name: 'bob',
        trusted: false,
        nodes: 1,
        nodeList: [
          NetConnNode(
            shortId: 'KooWbob',
            label: 'bob-desk',
            active: true,
            state: 'linked',
          ),
        ],
      ),
    ],
  );
  List<NetExchangeInfo> exchanges = const [];
  List<NetRemoteItem> remoteItems = const [];
  NetShards shards = const NetShards();
  List<NetSearchOrigin> searchHits = const [
    NetSearchOrigin(origin: 'local', lines: ['BTC/USD 5m match']),
  ];

  @override
  Future<NetConfig> netConfig() async => netCfg;

  @override
  Future<String> netConfigSet(NetConfig cfg) async {
    netCalls.add('config-set:${cfg.label}');
    netCfg = cfg;
    return 'saved network settings';
  }

  @override
  Future<NetConnections> netConnections() async => connections;

  @override
  Future<String> netConnectionTrust(String user, bool trusted) async {
    netCalls.add('trust:$user:$trusted');
    connections = NetConnections(
      me: connections.me,
      connections: [
        for (final c in connections.connections)
          if (c.userId == user || c.shortId == user || c.name == user)
            NetConnectionInfo(
              userId: c.userId,
              shortId: c.shortId,
              name: c.name,
              trusted: trusted,
              nodes: c.nodes,
              since: c.since,
              nodeList: c.nodeList,
            )
          else
            c,
      ],
    );
    return trusted ? '$user is now trusted' : '$user is now untrusted';
  }

  @override
  Future<String> netConnectionRemove(String user) async {
    netCalls.add('remove:$user');
    connections = NetConnections(
      me: connections.me,
      connections: [
        for (final c in connections.connections)
          if (c.userId != user && c.shortId != user && c.name != user) c,
      ],
    );
    return 'connection with $user removed';
  }

  @override
  Future<List<NetExchangeInfo>> netExchanges() async => List.of(exchanges);

  @override
  Future<String> netExchangeLabel(String exchange, String label) async {
    netCalls.add('exchange-label:$exchange:$label');
    exchanges = [
      for (final x in exchanges)
        if (x.id == exchange || x.shortId == exchange)
          NetExchangeInfo(
            id: x.id,
            shortId: x.shortId,
            title: x.title,
            label: label,
            status: x.status,
            role: x.role,
            peerId: x.peerId,
            peerLabel: x.peerLabel,
            shortCode: x.shortCode,
            fingerprint: x.fingerprint,
            expiresAt: x.expiresAt,
            verifiedAt: x.verifiedAt,
            revokedAt: x.revokedAt,
            createdAt: x.createdAt,
            active: x.active,
            incoming: x.incoming,
          )
        else
          x,
    ];
    return 'exchange $exchange is now $label';
  }

  @override
  Future<String> netExchangeVerify(String exchange, String code) async {
    netCalls.add('exchange-verify:$exchange:$code');
    return 'key exchange with $exchange verified';
  }

  @override
  Future<String> netExchangeRevoke(String exchange) async {
    netCalls.add('exchange-revoke:$exchange');
    exchanges = [
      for (final x in exchanges)
        if (x.id != exchange && x.shortId != exchange) x,
    ];
    return 'disconnected from $exchange';
  }

  @override
  Future<List<NetRemoteItem>> netRemoteList(String exchange) async {
    netCalls.add('remote-list:$exchange');
    return List.of(remoteItems);
  }

  @override
  Future<String> netFetch(
    String exchange,
    String item, {
    String? format,
    String? path,
    bool import = false,
  }) async {
    netCalls.add('fetch:$exchange:$item:${import ? "import" : path ?? ""}');
    if (import) {
      return 'imported 1';
    }
    return 'wrote ${path ?? "file"}';
  }

  @override
  Future<NetShards> netShards() async => shards;

  @override
  Future<String> netShardEntrust({
    required List<String> to,
    required String keyFile,
    required String shardPassword,
    String? filePassword,
    int threshold = 0,
  }) async {
    netCalls.add('shard-entrust:${to.join(",")}:$keyFile:$threshold');
    return 'entrusted: any ${threshold == 0 ? 2 : threshold} of ${to.length} shards restore your key';
  }

  @override
  Future<String> netShardCheck(String shardPassword) async {
    netCalls.add('shard-check');
    return "your shards restore this database's key";
  }

  @override
  Future<String> netShardDrop(String id) async {
    netCalls.add('shard-drop:$id');
    shards = NetShards(
      entrusted: shards.entrusted,
      held: [
        for (final h in shards.held)
          if (h.id != id && h.shortId != id) h,
      ],
    );
    return 'dropped';
  }

  @override
  Future<List<NetSearchOrigin>> netSearch(
    String saved, {
    List<String> only = const [],
    bool remoteOnly = false,
    int limit = 200,
  }) async {
    netCalls.add(
      'search:$saved:${remoteOnly ? "remote" : "all"}'
      '${only.isEmpty ? '' : ':only=${only.join(",")}'}',
    );
    return List.of(searchHits);
  }

  @override
  Future<String> netBroadcast(
    String topic,
    String text, {
    String to = 'nodes',
  }) async {
    netCalls.add('broadcast:$to:$topic:$text');
    return 'sent to 0 node(s)';
  }

  NetExchangeBackupPeek exchangePeek = const NetExchangeBackupPeek(
    peerId: '12D3KooWPeer',
    exchanges: 1,
  );

  @override
  Future<String> netExchangeExport({
    required String out,
    required String password,
  }) async {
    if (password.isEmpty) {
      throw 'export password must not be empty';
    }
    netCalls.add('exchange-export:$out');
    return 'wrote $out (0 exchange(s), age-encrypted)';
  }

  @override
  Future<NetExchangeBackupPeek> netExchangeRestorePeek({
    required String file,
    required String password,
  }) async {
    if (password.isEmpty) {
      throw 'export password must not be empty';
    }
    netCalls.add('exchange-restore-peek:$file');
    return exchangePeek;
  }

  @override
  Future<String> netExchangeRestore({
    required String file,
    required String password,
    bool confirmReplace = false,
  }) async {
    if (password.isEmpty) {
      throw 'export password must not be empty';
    }
    if (exchangePeek.identityReplaced && !confirmReplace) {
      throw "this database's node identity would become ${exchangePeek.peerId}; confirm to continue";
    }
    netCalls.add('exchange-restore:$file:$confirmReplace');
    return 'restored ${exchangePeek.exchanges} key exchange(s) for node ${exchangePeek.peerId}';
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

  final List<OrgDomainInfo> domainList = [];
  final Map<String, OrgTemplateInfo> templates = {};
  OrgCertOverview certs = const OrgCertOverview(
    node: OrgNodeCertInfo(source: 'self-signed', selfSignedDays: 7),
  );

  OrgInfo _org(String key) => orgs.mine.firstWhere(
    (o) => o.id == key || o.slug == key || o.name == key,
    orElse: () => throw 'no organization `$key`',
  );

  void _putOrg(OrgInfo next) {
    orgs = OrgOverview(
      mine: [
        for (final o in orgs.mine)
          if (o.id == next.id) next else o,
      ],
      joined: orgs.joined,
      mirrors: orgs.mirrors,
    );
  }

  OrgInfo _copyOrg(
    OrgInfo o, {
    List<String>? templates,
    String? template,
    List<OrgEndpointInfo>? endpoints,
    List<String>? projects,
  }) => OrgInfo(
    id: o.id,
    shortId: o.shortId,
    name: o.name,
    slug: o.slug,
    description: o.description,
    visibility: o.visibility,
    access: o.access,
    hosted: o.hosted,
    memberAuth: o.memberAuth,
    gated: o.gated,
    members: o.members,
    applyRole: o.applyRole,
    inviteRole: o.inviteRole,
    template: template ?? o.template,
    roles: o.roles,
    memberList: o.memberList,
    resources: o.resources,
    projects: projects ?? o.projects,
    endpoints: endpoints ?? o.endpoints,
    templates: templates ?? o.templates,
  );

  @override
  Future<List<OrgTemplateInfo>> orgTemplates(String org) async => [
    for (final n in _org(org).templates)
      templates[n] ?? OrgTemplateInfo(name: n),
  ];

  @override
  Future<OrgTemplateInfo> orgTemplateShow(String org, String template) async {
    orgCalls.add('template-show:$org:$template');
    return templates[template] ??
        OrgTemplateInfo(name: template, html: '<h1>{{title}}</h1>');
  }

  @override
  Future<OrgTemplateInfo> orgTemplateSave(
    String org,
    String name, {
    String html = '',
    String css = '',
    bool useIt = false,
  }) async {
    orgCalls.add('template-save:$org:$name');
    final t = OrgTemplateInfo(
      name: name,
      inUse: useIt,
      html: html.isEmpty ? '<h1>{{title}}</h1>' : html,
      css: css,
    );
    templates[name] = t;
    final o = _org(org);
    final names = [...o.templates.where((n) => n != name), name];
    _putOrg(_copyOrg(o, templates: names, template: useIt ? name : o.template));
    return t;
  }

  @override
  Future<String> orgTemplateRemove(String org, String template) async {
    orgCalls.add('template-remove:$org:$template');
    templates.remove(template);
    final o = _org(org);
    _putOrg(
      _copyOrg(
        o,
        templates: [
          for (final n in o.templates)
            if (n != template) n,
        ],
        template: o.template == template ? 'default' : o.template,
      ),
    );
    return 'template $template removed';
  }

  @override
  Future<String> orgTemplateAddAsset(
    String org,
    String template,
    String path, {
    String? name,
  }) async {
    orgCalls.add('template-asset:$org:$template:$path');
    return 'asset ${name ?? path} (0 bytes) at /assets/';
  }

  @override
  Future<String> orgTemplateRemoveAsset(
    String org,
    String template,
    String name,
  ) async {
    orgCalls.add('template-asset-rm:$org:$template:$name');
    return 'asset $name removed from $template';
  }

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
  }) async {
    orgCalls.add('template-build:$org:$template');
    return clear
        ? 'build step of $template removed'
        : 'built $template: index.html';
  }

  @override
  Future<String> orgTemplatePreview(String org, String template) async {
    orgCalls.add('template-preview:$org:$template');
    return '/o/$org/preview/$template (the site is not running here; start it to serve this preview for an hour)';
  }

  @override
  Future<List<OrgDomainInfo>> orgDomains({String? org}) async => [
    for (final d in domainList)
      if (org == null || d.org == org) d,
  ];

  @override
  Future<String> orgDomainAdd(String org, String domain) async {
    orgCalls.add('domain-add:$org:$domain');
    domainList.add(OrgDomainInfo(org: org, domain: domain));
    return '$org is served at $domain';
  }

  @override
  Future<String> orgDomainRemove(String org, String domain) async {
    orgCalls.add('domain-rm:$org:$domain');
    domainList.removeWhere((d) => d.org == org && d.domain == domain);
    return '$org is no longer served at $domain';
  }

  @override
  Future<OrgCertOverview> orgCertStatus({String? org}) async => certs;

  @override
  Future<String> orgCertSet(String org, String cert, String key) async {
    orgCalls.add('cert-set:$org:$cert');
    return '$org: your certificate set';
  }

  @override
  Future<String> orgCertClear(String org) async {
    orgCalls.add('cert-clear:$org');
    return '$org: back to a self-signed certificate';
  }

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
  }) async {
    orgCalls.add('cert-acme:${org ?? 'node'}:${off ? 'off' : 'on'}');
    return off ? 'ACME off' : 'ACME on for ${org ?? domain ?? 'node'}';
  }

  @override
  Future<String> orgCertRenew({String? org}) async {
    orgCalls.add('cert-renew:${org ?? 'node'}');
    return 'nothing to renew';
  }

  @override
  Future<List<OrgBrowseItem>> orgBrowse(String target, {String? org}) async {
    orgCalls.add('browse:$target:${org ?? ''}');
    if (target.trim().isEmpty) {
      throw 'browse a node (…/p2p/ID or a known peer)';
    }
    return const [
      OrgBrowseItem(
        name: 'Peer Lab',
        slug: 'peer-lab',
        visibility: 'public',
        access: 'open',
        members: 3,
        description: 'elsewhere',
      ),
    ];
  }

  @override
  Future<OrgFetchResult> orgFetch(
    String org,
    String resource, {
    String? format,
    String? out,
  }) async {
    orgCalls.add('fetch:$org:$resource');
    return OrgFetchResult(
      name: resource,
      format: format ?? 'json',
      size: 2,
      text: '{}',
    );
  }

  @override
  Future<String> orgGive(String org, String kind, String name) async {
    orgCalls.add('give:$org:$kind:$name');
    return 'gave $kind $name to $org (yours stays as it is)';
  }

  @override
  Future<String> orgMirror(String org) async {
    orgCalls.add('mirror:$org');
    return 'mirrored $org (0 bytes); start the site to serve it at /o/$org/';
  }

  @override
  Future<List<OrgSyncCodeItem>> orgSyncCode(String org) async {
    orgCalls.add('sync-code:$org');
    return const [OrgSyncCodeItem(name: 'parser', state: 'same')];
  }

  @override
  Future<String> orgSignIn(String org, String site) async {
    orgCalls.add('sign-in:$org:$site');
    if (site.trim().isEmpty) {
      throw "give the site's address, e.g. https://lab.example.org:4743";
    }
    return '$site/o/$org/sign-in?auth=demo';
  }

  @override
  Future<String> orgEndpointAdd(
    String org,
    String name,
    String kind,
    String target, {
    String? perm,
    bool anyone = false,
  }) async {
    orgCalls.add('endpoint-add:$org:$name:$kind:$target');
    final o = _org(org);
    _putOrg(
      _copyOrg(
        o,
        endpoints: [
          ...o.endpoints,
          OrgEndpointInfo(
            name: name,
            kind: kind,
            perm: perm ?? 'call',
            anyone: anyone,
          ),
        ],
      ),
    );
    return 'endpoint $name ($kind, needs ${perm ?? 'call'})';
  }

  @override
  Future<String> orgEndpointRemove(String org, String name) async {
    orgCalls.add('endpoint-rm:$org:$name');
    final o = _org(org);
    _putOrg(
      _copyOrg(
        o,
        endpoints: [
          for (final e in o.endpoints)
            if (e.name != name) e,
        ],
      ),
    );
    return 'removed endpoint $name';
  }

  @override
  Future<String> orgCall(
    String org,
    String endpoint, {
    String input = '{}',
  }) async {
    orgCalls.add('call:$org:$endpoint');
    return '{ "ok": true }';
  }

  @override
  Future<String> orgProjectAdd(String org, String project) async {
    orgCalls.add('project-add:$org:$project');
    final o = _org(org);
    _putOrg(_copyOrg(o, projects: [...o.projects, project]));
    return '$project is a project of ${o.name}';
  }

  @override
  Future<String> orgProjectRemove(String org, String project) async {
    orgCalls.add('project-rm:$org:$project');
    final o = _org(org);
    _putOrg(
      _copyOrg(
        o,
        projects: [
          for (final n in o.projects)
            if (n != project) n,
        ],
      ),
    );
    return '$project is no longer a project of ${o.name} (the project stays here)';
  }

  @override
  Future<String> orgProjectMember(
    String org,
    String project,
    String member, {
    String? role,
  }) async {
    orgCalls.add('project-member:$org:$project:$member');
    return '$member is on $project';
  }

  @override
  Future<List<OrgCodeServedInfo>> orgCodeList(String org) async => const [];

  @override
  Future<String> orgCodeServe(String org, String space) async {
    orgCalls.add('code-serve:$org:$space');
    return 'this node serves $space for $org';
  }

  @override
  Future<String> orgCodeStop(String org, String space) async {
    orgCalls.add('code-stop:$org:$space');
    return 'this node no longer serves $space for $org';
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
