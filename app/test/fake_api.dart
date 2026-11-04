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
        name: 'Muninn', version: '2.21.0', tagline: 'Local, encrypted store for OHLCV market bars.',
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

  @override
  Future<Overview> overview(String password) async {
    lastPassword = password;
    return const Overview(
      projects: [Project(shortId: 'abc123', name: 'Alpha', description: 'test project')],
      datasets: [Dataset(source: 'coinbase', base: 'BTC', quote: 'USD', duration: '5m', count: 288, first: 1704067200000, last: 1704153300000)],
    );
  }
}
