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
  const Project({required this.shortId, required this.name, required this.description});
  final String shortId, name, description;
}

class Dataset {
  const Dataset({required this.source, required this.base, required this.quote, required this.duration, required this.count, required this.first, required this.last});
  final String source, base, quote, duration;
  final int count, first, last;
  String get symbol => '$base/$quote';
}

class Overview {
  const Overview({required this.projects, required this.datasets});
  final List<Project> projects;
  final List<Dataset> datasets;
}

abstract class MuninnApi {
  About about();
  Future<List<String>> listThemes();
  Future<MuninnTheme> loadTheme(String name, String variant);
  Future<({String name, String mode})> currentTheme();
  Future<DataDir> dataDir();
  /// Read-only: the store is opened without migrations or writes.
  Future<Overview> overview(String password);
}
