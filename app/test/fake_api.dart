import 'package:muninn/src/api.dart';

const tokyoDark = {
  'bg': '#1a1b26', 'fg': '#c0caf5', 'muted': '#565f89', 'border': '#3b4261', 'border_focus': '#7aa2f7',
  'selection_bg': '#283457', 'selection_fg': '#c0caf5', 'accent': '#bb9af7', 'header': '#7aa2f7',
  'up': '#9ece6a', 'down': '#f7768e', 'warning': '#e0af68', 'error': '#f7768e', 'success': '#9ece6a',
  'info': '#7dcfff', 'chart_1': '#7aa2f7', 'chart_2': '#bb9af7', 'chart_3': '#7dcfff', 'chart_4': '#ff9e64',
  'chart_5': '#9ece6a', 'chart_6': '#e0af68',
};
const nordLight = {'bg': '#eceff4', 'fg': '#2e3440', 'header': '#5e81ac', 'accent': '#b48ead', 'border': '#d8dee9'};

class FakeApi implements MuninnApi {
  final loaded = <String>[];
  String? lastPassword;

  @override
  About about() => const About(
        name: 'Muninn', version: '2.21.0', tagline: 'Local, encrypted workbench for market data with peer-to-peer data workflows.',
        motto: "Odin's raven of memory: it remembers every bar.", publisher: 'WeirdBrains',
        homepage: 'https://muninn.weirdbrains.com', repository: 'https://github.com/WeirdBrains/muninn',
        license: 'BSD-3-Clause', copyright: '© 2026', credits: []);

  @override
  Future<List<String>> listThemes() async => ['tokyo-night', 'nord'];

  @override
  Future<MuninnTheme> loadTheme(String name, String variant) async {
    loaded.add('$name/$variant');
    return MuninnTheme(name: name, displayName: name, variant: variant, roles: name == 'nord' ? nordLight : tokyoDark);
  }

  @override
  Future<({String name, String mode})> currentTheme() async => (name: 'tokyo-night', mode: 'dark');

  @override
  Future<DataDir> dataDir() async => const DataDir(home: '/tmp/muninn-test', database: '/tmp/muninn-test/storage/muninn.database', databaseExists: true, keysInitialized: true);

  SavedState saved = const SavedState(mode: 'dark');
  final saves = <String>[];
  @override
  Future<SavedState> savedState() async => saved;
  @override
  Future<void> saveTheme(String name) async => saves.add('theme=$name');
  @override
  Future<void> saveThemeMode(String mode) async => saves.add('mode=$mode');
  @override
  Future<void> saveTimeRange(TimeRange r) async => saves.add('range=${r.from}..${r.to}');
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

  List<Project> projects = const [Project(id: 'p1', shortId: 'abc123', name: 'Alpha', description: 'test project')];
  final Map<String, List<ProjectSeries>> seriesByProject = {
    'p1': [const ProjectSeries(itemId: 1, source: 'coinbase', base: 'BTC', quote: 'USD', durationS: 300, from: 1704067200000)],
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
          Dataset(source: 'coinbase', base: 'BTC', quote: 'USD', duration: '5m', durationS: 300, count: 288, first: 1704067200000, last: 1704153300000),
          Dataset(source: 'binance', base: 'ETH', quote: 'USDT', duration: '1h', durationS: 3600, count: 24, first: 1609459200000, last: 1609542000000),
        ],
      );

  @override
  Future<List<ProjectSeries>> projectSeries(String projectId) async =>
      List<ProjectSeries>.from(seriesByProject[projectId] ?? const []);

  @override
  Future<Project> createProject(String name, {String description = ''}) async {
    if (projectError != null) throw projectError!;
    createdProjects.add(name);
    final p = Project(id: 'p$nextProject', shortId: 'new$nextProject', name: name, description: description);
    nextProject++;
    projects = [...projects, p];
    seriesByProject[p.id] = [];
    return p;
  }

  @override
  Future<Project> updateProject(String id, {String? name, String? description}) async {
    if (projectError != null) throw projectError!;
    updatedProjects.add((id: id, name: name, description: description));
    final i = projects.indexWhere((p) => p.id == id);
    if (i < 0) throw 'no such project';
    final cur = projects[i];
    final p = Project(id: cur.id, shortId: cur.shortId, name: name ?? cur.name, description: description ?? cur.description);
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
  Future<bool> addProjectSeries(String projectId, Dataset d, TimeRange range) async {
    if (projectError != null) throw projectError!;
    addedSeries.add((projectId: projectId, d: d, range: range));
    final list = List<ProjectSeries>.from(seriesByProject[projectId] ?? const []);
    if (list.any((s) => s.source == d.source && s.base == d.base && s.quote == d.quote && s.durationS == d.durationS)) {
      return false;
    }
    list.add(ProjectSeries(
      itemId: nextItemId++,
      source: d.source,
      base: d.base,
      quote: d.quote,
      durationS: d.durationS,
      from: range.from,
      to: range.to,
    ));
    seriesByProject[projectId] = list;
    return true;
  }

  @override
  Future<void> removeProjectSeries(int itemId) async {
    if (projectError != null) throw projectError!;
    removedSeries.add(itemId);
    for (final id in seriesByProject.keys.toList()) {
      seriesByProject[id] = seriesByProject[id]!.where((s) => s.itemId != itemId).toList();
    }
  }

  @override
  Future<List<BarPoint>> queryBars(Dataset d, TimeRange range, {int maxPoints = 600}) async {
    queries.add(range);
    return [
      for (var i = 0; i < 50; i++)
        BarPoint(ts: d.first + i * d.durationS * 1000, open: 100.0 + i, high: 102.0 + i, low: 99.0 + i, close: i.isEven ? 101.0 + i : 99.5 + i, volume: 10.0 + i),
    ];
  }

  final ingests = <({String path, String? source})>[];
  final exports = <({String path, String format, String? source, String? symbol, String? duration, String? from, String? to})>[];
  IngestResult nextIngest = const IngestResult(files: [
    IngestFileReport(locator: '/tmp/a.csv', source: 'math', format: 'csv', bytes: 8, id: 'abc', skipped: false, elapsedMs: 12),
  ]);
  ExportResult nextExport = const ExportResult(path: '/tmp/out.csv', format: 'csv', rows: 3);
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
    exports.add((path: path, format: format, source: source, symbol: symbol, duration: duration, from: from, to: to));
    if (exportError != null) throw exportError!;
    return nextExport;
  }

  NetOverview net = const NetOverview(
    status: NetStatus(label: 'muninn'),
    peers: [NetPeerInfo(peerId: '12D3KooWtestpeer', shortId: 'estpeer', label: 'lab', caps: 'muninn', source: 'mdns', lastSeen: 1704067200000)],
  );

  @override
  Future<NetOverview> netOverview() async => net;
}
