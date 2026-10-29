//! clap definitions for `mnnn`.
use crate::prefs::ThemeMode;
use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "mnnn",
    version,
    about = "Muninn: ingest and normalize crypto market data into bars",
    disable_help_subcommand = true,
    after_help = "Docs: `mnnn help` (topics), `mnnn help install` (install and uninstall per platform)."
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, Args)]
pub struct GlobalArgs {
    /// Muninn home directory (default ~/.muninn).
    #[arg(long, global = true, env = "MUNINN_HOME", value_name = "DIR")]
    pub home: Option<PathBuf>,
    /// Database file (default <home>/storage/muninn.database).
    #[arg(long, global = true, env = "MUNINN_DB", value_name = "PATH")]
    pub db: Option<PathBuf>,
    /// Never prompt; missing values are errors.
    #[arg(long, global = true, env = "MUNINN_NON_INTERACTIVE")]
    pub non_interactive: bool,
    /// More output (repeatable).
    #[arg(short, long, global = true, action = clap::ArgAction::Count, conflicts_with = "quiet")]
    pub verbose: u8,
    /// Print nothing but errors and requested data.
    #[arg(short, long, global = true, env = "MUNINN_QUIET")]
    pub quiet: bool,
    /// Answer yes to every confirmation.
    #[arg(short, long, global = true, env = "MUNINN_YES")]
    pub yes: bool,
    /// Override the theme mode for this run.
    #[arg(long, global = true, env = "MUNINN_THEME_MODE", value_enum)]
    pub theme_mode: Option<ThemeMode>,
}

#[derive(Debug, Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum Command {
    /// Ingest bars from files or services.
    #[command(subcommand)]
    Ingest(IngestCommand),
    /// Export bars.
    Export(ExportArgs),
    /// Manage sources.
    #[command(subcommand)]
    Sources(SourcesCommand),
    /// Manage the known symbol list.
    #[command(subcommand)]
    Symbols(SymbolsCommand),
    /// Key management.
    #[command(subcommand)]
    Keys(KeysCommand),
    /// Backup and restore.
    #[command(subcommand)]
    Backup(BackupCommand),
    /// Themes.
    #[command(subcommand)]
    Theme(ThemeCommand),
    /// Authors: who actions are attributed to (stored in the encrypted database).
    #[command(subcommand, alias = "authors")]
    Author(AuthorCommand),
    /// Action history: list, inspect and step through what was done (forks are kept).
    #[command(subcommand)]
    History(HistoryCommand),
    /// Live searches: saved searches that always run on the current data.
    #[command(subcommand, alias = "searches")]
    Search(SearchCommand),
    /// Projects: collections of live searches, results and journals.
    #[command(subcommand, alias = "projects")]
    Project(ProjectCommand),
    /// Journals: entries in Markdown or LaTeX with images, notes and highlights.
    #[command(subcommand, alias = "journals")]
    Journal(JournalCommand),
    /// Launch the terminal UI.
    #[command(after_long_help = TUI_NOTES)]
    Tui,
    /// Show a help topic (the same pages as the TUI help and docs/).
    #[command(after_long_help = HELP_NOTES)]
    Help {
        /// Topic id, e.g. keys, services/redis.
        topic: Option<String>,
        /// List topics matching a query.
        #[arg(long)]
        search: Option<String>,
    },
    /// Generate man pages.
    #[command(hide = true)]
    Man {
        #[arg(long, default_value = "man")]
        out: PathBuf,
    },
}

/// Values shared by every ingest kind.
#[derive(Debug, Clone, Args, Default)]
pub struct CommonIngestArgs {
    /// Where the data came from, e.g. coinbase (created if unknown). Default: an exchange
    /// named in the file name (binance_…, coinbase_…), else a prompt.
    #[arg(long)]
    pub source: Option<String>,
    /// Symbol, e.g. BTC/USDT, BTC-USD or BTCUSDT.
    #[arg(long)]
    pub symbol: Option<String>,
    /// Bar duration, e.g. 5m, 1h, 1d.
    #[arg(long)]
    pub duration: Option<String>,
    /// Column mapping `<col>=<field>` (fields: timestamp, open, high, low, close, volume). Repeatable.
    #[arg(long = "map", value_name = "COL=FIELD")]
    pub map: Vec<String>,
    /// Write rejected rows (with reasons) to this CSV.
    #[arg(long, value_name = "FILE")]
    pub rejects: Option<PathBuf>,
    /// Re-process even if this exact input was already ingested.
    #[arg(long)]
    pub force: bool,
    /// For a directory --path: only its top level (default: search subdirectories too).
    #[arg(long)]
    pub no_recursive: bool,
}

#[derive(Debug, Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum IngestCommand {
    /// Ingest CSV file(s).
    Csv(CsvIngestArgs),
    /// Ingest JSON / NDJSON file(s).
    Json(JsonIngestArgs),
    /// Ingest a whole path: every CSV and JSON file in a directory tree (or one file).
    Path(PathIngestArgs),
    /// Ingest from a service.
    #[command(subcommand)]
    Service(ServiceCommand),
    /// Show import runs (status, cancels) and their artifact ledger.
    Runs(RunsArgs),
}

#[derive(Debug, Clone, Args)]
pub struct RunsArgs {
    /// Show one run with its artifacts (default: the latest run).
    #[arg(long)]
    pub show: Option<i64>,
}

#[derive(Debug, Clone, Args)]
pub struct CsvIngestArgs {
    /// File or directory of CSV files.
    #[arg(long, short = 'p')]
    pub path: PathBuf,
    #[command(flatten)]
    pub common: CommonIngestArgs,
    #[arg(long, default_value_t = ',')]
    pub delimiter: char,
    #[arg(long, default_value_t = '"')]
    pub quote: char,
    #[arg(long)]
    pub escape: Option<char>,
    /// The file has no header row.
    #[arg(long)]
    pub no_header: bool,
    #[arg(long, default_value_t = 0)]
    pub skip_rows: usize,
}

#[derive(Debug, Clone, Args)]
pub struct JsonIngestArgs {
    /// File or directory of .json / .ndjson / .jsonl files.
    #[arg(long, short = 'p')]
    pub path: PathBuf,
    #[command(flatten)]
    pub common: CommonIngestArgs,
    /// Where the array of bars lives (dotted path or JSON pointer).
    #[arg(long)]
    pub records: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct PathIngestArgs {
    /// File or directory; directories are searched recursively for .csv/.tsv/.txt and
    /// .json/.ndjson/.jsonl files (hidden entries skipped).
    #[arg(value_name = "PATH", required_unless_present = "path")]
    pub target: Option<PathBuf>,
    /// Same as the positional PATH.
    #[arg(long, short = 'p', conflicts_with = "target")]
    pub path: Option<PathBuf>,
    #[command(flatten)]
    pub common: CommonIngestArgs,
    /// CSV field delimiter.
    #[arg(long, default_value_t = ',')]
    pub delimiter: char,
    /// CSV files have no header row.
    #[arg(long)]
    pub no_header: bool,
    /// JSON: where the array of bars lives (dotted path or JSON pointer).
    #[arg(long)]
    pub records: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum ServiceCommand {
    Http(HttpArgs),
    Redis(RedisArgs),
    Sqlite(SqliteArgs),
    Postgres(PostgresArgs),
}

/// Flags shared by every service.
#[derive(Debug, Clone, Args)]
pub struct ServiceCommon {
    #[command(flatten)]
    pub ingest: CommonIngestArgs,
    /// Timeout in seconds.
    #[arg(long, default_value_t = 30)]
    pub timeout: u64,
}

/// Secret input options.
#[derive(Debug, Clone, Args, Default)]
pub struct PasswordArgs {
    /// Password (prefer --password-env / --password-stdin: flags end up in shell history).
    #[arg(long)]
    pub password: Option<String>,
    /// Read the password from this environment variable.
    #[arg(long, value_name = "VAR", conflicts_with = "password")]
    pub password_env: Option<String>,
    /// Read the password from stdin.
    #[arg(long, conflicts_with_all = ["password", "password_env"])]
    pub password_stdin: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
}

impl HttpMethod {
    pub fn allows_body(self) -> bool {
        matches!(self, HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch)
    }
    pub fn as_str(self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Head => "HEAD",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DataFormat {
    Json,
    Csv,
    Ndjson,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum Paginate {
    #[default]
    None,
    Cursor,
    Time,
}

#[derive(Debug, Clone, Args)]
pub struct HttpArgs {
    #[arg(long)]
    pub url: String,
    #[arg(short = 'X', long, value_enum, ignore_case = true, default_value = "get")]
    pub method: HttpMethod,
    /// Request body, or @FILE. Only with POST/PUT/PATCH.
    #[arg(long)]
    pub body: Option<String>,
    /// Query parameter KEY=VALUE (repeatable, URL-encoded).
    #[arg(long = "param", value_name = "KEY=VALUE")]
    pub params: Vec<String>,
    /// Header "KEY: VALUE" (repeatable, overrides defaults).
    #[arg(short = 'H', long = "header", value_name = "KEY: VALUE")]
    pub headers: Vec<String>,
    /// Response format (default: from Content-Type).
    #[arg(long, value_enum)]
    pub format: Option<DataFormat>,
    /// Where the array of bars lives in the JSON response.
    #[arg(long)]
    pub records: Option<String>,
    #[arg(long, value_enum, default_value = "none")]
    pub paginate: Paginate,
    /// cursor pagination: path of the next cursor in the response.
    #[arg(long, default_value = "next")]
    pub cursor_path: String,
    /// cursor pagination: query parameter that carries the cursor.
    #[arg(long, default_value = "cursor")]
    pub cursor_param: String,
    /// time pagination: query parameter for the window start (epoch ms).
    #[arg(long, default_value = "startTime")]
    pub start_param: String,
    /// time pagination: start time.
    #[arg(long)]
    pub from: Option<String>,
    /// time pagination: end time.
    #[arg(long)]
    pub to: Option<String>,
    #[arg(long, default_value_t = 1000)]
    pub max_pages: usize,
    /// Requests per second.
    #[arg(long)]
    pub rate_limit: Option<f64>,
    #[arg(long, default_value_t = 3)]
    pub retries: u32,
    /// Skip TLS verification (insecure).
    #[arg(long)]
    pub insecure: bool,
    #[command(flatten)]
    pub common: ServiceCommon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum RedisType {
    #[default]
    Auto,
    Stream,
    List,
    Zset,
    Hash,
    String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum ValueFormat {
    #[default]
    Json,
    Csv,
    Fields,
}

#[derive(Debug, Clone, Args)]
#[command(group(ArgGroup::new("target").required(true).args(["key", "pattern"])))]
pub struct RedisArgs {
    /// Host or redis:// / rediss:// URI.
    #[arg(long)]
    pub uri: String,
    #[arg(long)]
    pub port: Option<u16>,
    #[arg(long)]
    pub username: Option<String>,
    #[command(flatten)]
    pub password: PasswordArgs,
    /// Logical database index (`--db` is taken by the global database-file flag).
    #[arg(long = "db-index", default_value_t = 0)]
    pub db_index: i64,
    #[arg(long)]
    pub tls: bool,
    #[arg(long)]
    pub key: Option<String>,
    #[arg(long)]
    pub pattern: Option<String>,
    #[arg(long = "type", value_enum, default_value = "auto")]
    pub kind: RedisType,
    #[arg(long, value_enum, default_value = "json")]
    pub value_format: ValueFormat,
    #[arg(long)]
    pub from: Option<String>,
    #[arg(long)]
    pub to: Option<String>,
    #[arg(long, default_value_t = 1000)]
    pub batch: usize,
    #[command(flatten)]
    pub common: ServiceCommon,
}

#[derive(Debug, Clone, Args)]
#[command(group(ArgGroup::new("what").required(true).args(["table", "query"])))]
pub struct SqliteArgs {
    #[arg(long)]
    pub path: PathBuf,
    #[arg(long)]
    pub table: Option<String>,
    /// SQL or @FILE.
    #[arg(long)]
    pub query: Option<String>,
    /// Key if the source database is SQLCipher-encrypted.
    #[arg(long)]
    pub key: Option<String>,
    #[command(flatten)]
    pub common: ServiceCommon,
}

#[derive(Debug, Clone, Args)]
#[command(group(ArgGroup::new("what").required(true).args(["table", "query"])))]
pub struct PostgresArgs {
    /// postgres://user:pass@host:port/db?sslmode=...
    #[arg(long, conflicts_with_all = ["username", "host", "port", "database", "secure"])]
    pub url: Option<String>,
    #[arg(long, required_unless_present = "url")]
    pub username: Option<String>,
    #[command(flatten)]
    pub password: PasswordArgs,
    #[arg(long, required_unless_present = "url")]
    pub host: Option<String>,
    #[arg(long)]
    pub port: Option<u16>,
    #[arg(long)]
    pub database: Option<String>,
    /// sslmode=require (otherwise prefer).
    #[arg(long)]
    pub secure: bool,
    /// CA certificate to verify the server (verify-full).
    #[arg(long)]
    pub ca_cert: Option<PathBuf>,
    #[arg(long, default_value = "public")]
    pub schema: String,
    #[arg(long)]
    pub table: Option<String>,
    /// SQL or @FILE.
    #[arg(long)]
    pub query: Option<String>,
    #[arg(long, default_value_t = 10000)]
    pub batch: usize,
    #[command(flatten)]
    pub common: ServiceCommon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ExportFormat {
    Csv,
    Parquet,
    Json,
}

#[derive(Debug, Clone, Args)]
pub struct ExportArgs {
    #[arg(long, value_enum)]
    pub format: ExportFormat,
    /// Output path, or `-` for stdout (csv/json).
    #[arg(long)]
    pub out: PathBuf,
    #[arg(long)]
    pub symbol: Option<String>,
    #[arg(long)]
    pub source: Option<String>,
    #[arg(long)]
    pub from: Option<String>,
    #[arg(long)]
    pub to: Option<String>,
    #[arg(long)]
    pub duration: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum SourcesCommand {
    List,
    Add {
        name: String,
        #[arg(long, default_value = "file")]
        kind: String,
        #[arg(long)]
        locator: Option<String>,
    },
    Rename {
        from: String,
        to: String,
    },
    Remove {
        name: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum SymbolsCommand {
    List,
    Add {
        /// e.g. BTC/USDT
        symbol: String,
        /// Aliases for the base asset, e.g. XBT.
        #[arg(long = "alias")]
        aliases: Vec<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum AuthorCommand {
    /// List authors (`*` marks the current one).
    List {
        /// Print JSON.
        #[arg(long)]
        json: bool,
    },
    /// Show one author (default: the current one).
    Show {
        /// Id, id prefix, name or email.
        author: Option<String>,
        /// Print JSON.
        #[arg(long)]
        json: bool,
    },
    /// Add an author. The first author becomes the current one.
    Add {
        /// Display name.
        #[arg(long)]
        name: String,
        #[arg(long)]
        email: Option<String>,
        /// Extra detail `key=value`, e.g. org=Acme. Repeatable.
        #[arg(long = "detail", value_name = "KEY=VALUE")]
        details: Vec<String>,
        /// Make this the current author.
        #[arg(long)]
        current: bool,
    },
    /// Change an author's name, email or details.
    Edit {
        /// Id, id prefix, name or email.
        author: String,
        #[arg(long)]
        name: Option<String>,
        /// New email; an empty value clears it.
        #[arg(long)]
        email: Option<String>,
        /// Set a detail `key=value`; `key=` removes it. Repeatable.
        #[arg(long = "detail", value_name = "KEY=VALUE")]
        details: Vec<String>,
        /// Replace all details with the ones given.
        #[arg(long)]
        replace_details: bool,
    },
    /// Remove an author (past actions keep their attribution id).
    #[command(alias = "rm")]
    Remove { author: String },
    /// Make an author the current one; `--none` clears it.
    Use {
        #[arg(required_unless_present = "none")]
        author: Option<String>,
        #[arg(long, conflicts_with = "author")]
        none: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum HistoryCommand {
    /// The current line of actions, newest first (`*` marks the head).
    List {
        /// Show at most this many actions.
        #[arg(long, short = 'n', default_value_t = 50)]
        limit: usize,
        /// Every action on every fork, newest first.
        #[arg(long)]
        all: bool,
        /// Print JSON.
        #[arg(long)]
        json: bool,
    },
    /// Show one action (default: the head).
    Show {
        id: Option<i64>,
        /// Print JSON.
        #[arg(long)]
        json: bool,
    },
    /// The whole history as a tree, forks indented under the action they branch from.
    Tree,
    /// Step the head back N actions, restoring the filters and theme from before them.
    Back {
        #[arg(default_value_t = 1)]
        steps: usize,
    },
    /// Step the head forward N actions (along the branch you stepped back from).
    Forward {
        #[arg(default_value_t = 1)]
        steps: usize,
    },
    /// Move the head to an action (`0` = before the first action).
    Goto { id: i64 },
}

/// What to search for. Options given replace that part of the search; the rest is kept.
#[derive(Debug, Clone, Args, Default)]
pub struct SpecArgs {
    /// Search text (fuzzy; words in any order), like the sidebar search.
    #[arg(long)]
    pub query: Option<String>,
    /// Search journal entries (title and text) instead of series.
    #[arg(long, conflicts_with = "series")]
    pub journals: bool,
    /// Search series (the default).
    #[arg(long)]
    pub series: bool,
    /// Only these sources. Repeatable.
    #[arg(long = "source", value_name = "NAME")]
    pub sources: Vec<String>,
    /// Only these symbols, e.g. BTC/USDT or btc-usdt. Repeatable.
    #[arg(long = "symbol", value_name = "SYMBOL")]
    pub symbols: Vec<String>,
    /// Only these bar periods, e.g. 5m, 1h. Repeatable.
    #[arg(long = "period", value_name = "DURATION")]
    pub periods: Vec<String>,
    /// Only series covering these years. Repeatable.
    #[arg(long = "year")]
    pub years: Vec<i32>,
    /// Time range start (date or date and time, UTC).
    #[arg(long, value_name = "TIME")]
    pub from: Option<String>,
    /// Time range end.
    #[arg(long, value_name = "TIME")]
    pub to: Option<String>,
    /// Price filter metric: open, high, low, close, typical, median, ohlc, vwap.
    #[arg(long, value_name = "METRIC")]
    pub price: Option<String>,
    /// Price filter minimum.
    #[arg(long)]
    pub min: Option<f64>,
    /// Price filter maximum.
    #[arg(long)]
    pub max: Option<f64>,
    /// Sort series by year or name.
    #[arg(long, value_parser = ["year", "name"])]
    pub sort: Option<String>,
    /// Sort descending.
    #[arg(long, conflicts_with = "asc")]
    pub desc: bool,
    /// Sort ascending.
    #[arg(long)]
    pub asc: bool,
    /// Start from the filters and sort the TUI shows now.
    #[arg(long)]
    pub from_view: bool,
    /// Start with no filters (edit and fork: drop the existing ones first).
    #[arg(long)]
    pub clear: bool,
}

#[derive(Debug, Subcommand)]
pub enum SearchCommand {
    /// List live searches with how many matches they have now.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Show a live search's definition.
    Show {
        /// Id, id prefix or name.
        search: String,
        #[arg(long)]
        json: bool,
    },
    /// Run a live search on the current data.
    #[command(alias = "results")]
    Run {
        search: String,
        #[arg(long)]
        json: bool,
    },
    /// Search once without saving (add --save NAME to keep it as a live search).
    Find {
        #[command(flatten)]
        spec: SpecArgs,
        /// Save this search as a live search with this name.
        #[arg(long, value_name = "NAME")]
        save: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Save a search as a live search.
    Save {
        name: String,
        #[command(flatten)]
        spec: SpecArgs,
    },
    /// Fork a live search: a linked copy, optionally changed by the options given.
    Fork {
        search: String,
        /// Name of the fork (default: "<name> (fork)").
        #[arg(long)]
        name: Option<String>,
        #[command(flatten)]
        spec: SpecArgs,
    },
    /// Refine a live search in place (rename it or change what it searches for).
    #[command(alias = "optimize")]
    Edit {
        search: String,
        #[arg(long)]
        name: Option<String>,
        #[command(flatten)]
        spec: SpecArgs,
    },
    /// Remove a live search (it also leaves every project).
    #[command(alias = "rm")]
    Remove { search: String },
}

#[derive(Debug, Subcommand)]
pub enum ProjectCommand {
    /// List projects.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Create a project.
    #[command(alias = "new")]
    Create {
        name: String,
        #[arg(long, default_value = "")]
        description: String,
    },
    /// Show a project: its live searches (with current matches), results and journals.
    Show {
        project: String,
        #[arg(long)]
        json: bool,
    },
    /// Rename a project or change its description.
    Edit {
        project: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
    },
    /// Remove a project. Its journals stay, as standalone journals.
    #[command(alias = "rm")]
    Remove { project: String },
    /// Add a live search, a journal or a result (a series, optionally a time range) to a project.
    #[command(group(ArgGroup::new("item").required(true).args(["search", "journal", "result"])))]
    Add {
        project: String,
        /// Live search id or name.
        #[arg(long)]
        search: Option<String>,
        /// Journal id or name.
        #[arg(long)]
        journal: Option<String>,
        /// A series as SOURCE:SYMBOL:PERIOD, e.g. binance:BTC/USDT:5m.
        #[arg(long, value_name = "SERIES")]
        result: Option<String>,
        /// Result time range start.
        #[arg(long, value_name = "TIME")]
        from: Option<String>,
        /// Result time range end.
        #[arg(long, value_name = "TIME")]
        to: Option<String>,
    },
    /// Take an item out of a project (the item itself is kept).
    Drop {
        project: String,
        /// Item number from `mnnn project show`.
        item: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum FormatArg {
    Markdown,
    Latex,
}

impl From<FormatArg> for crate::store::EntryFormat {
    fn from(f: FormatArg) -> Self {
        match f {
            FormatArg::Markdown => crate::store::EntryFormat::Markdown,
            FormatArg::Latex => crate::store::EntryFormat::Latex,
        }
    }
}

/// Entry text from an option, a file or stdin (`-`).
#[derive(Debug, Clone, Args, Default)]
pub struct TextArgs {
    /// The text.
    #[arg(long, conflicts_with = "file")]
    pub text: Option<String>,
    /// Read the text from a file (`-`: stdin).
    #[arg(long)]
    pub file: Option<PathBuf>,
}

/// Selected words of an entry.
#[derive(Debug, Clone, Args, Default)]
pub struct SelectArgs {
    /// The words to select (their first occurrence, or --nth).
    #[arg(long, value_name = "WORDS", conflicts_with = "range")]
    pub on: Option<String>,
    /// Which occurrence of --on (1 = first).
    #[arg(long, default_value_t = 1, requires = "on")]
    pub nth: usize,
    /// Character range START..END of the entry text.
    #[arg(long, value_name = "START..END")]
    pub range: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum JournalCommand {
    /// List journals (standalone and in projects).
    List {
        /// Only journals of this project.
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Create a journal, standalone or in a project.
    #[command(alias = "create")]
    New {
        name: String,
        #[arg(long)]
        project: Option<String>,
        /// Format for new entries.
        #[arg(long, value_enum, default_value = "markdown")]
        format: FormatArg,
    },
    /// Show a journal: notes, entries, highlights and images (--json: the whole document).
    Show {
        journal: String,
        #[arg(long)]
        json: bool,
    },
    /// Rename a journal or change the format for new entries.
    Edit {
        journal: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, value_enum)]
        format: Option<FormatArg>,
    },
    /// Remove a journal with its entries, notes, highlights and images.
    #[command(alias = "rm")]
    Remove { journal: String },
    /// Fork a journal (everything in it) into a project-specific journal.
    Fork {
        journal: String,
        /// Project for the fork (none: a standalone fork).
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        name: Option<String>,
    },
    /// Write a new entry.
    Write {
        journal: String,
        #[arg(long, default_value = "")]
        title: String,
        /// Entry format (default: the journal's).
        #[arg(long, value_enum)]
        format: Option<FormatArg>,
        #[command(flatten)]
        text: TextArgs,
    },
    /// Show one entry with its notes, highlights and images.
    Entry {
        entry: String,
        #[arg(long)]
        json: bool,
    },
    /// Change an entry's title, text or format (notes and highlights follow their words).
    EditEntry {
        entry: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long, value_enum)]
        format: Option<FormatArg>,
        #[command(flatten)]
        text: TextArgs,
    },
    /// Remove an entry.
    RemoveEntry { entry: String },
    /// Add selected entries (copies) to another journal or a new one.
    #[command(group(ArgGroup::new("target").required(true).args(["to", "new"])))]
    Copy {
        #[arg(required = true)]
        entries: Vec<String>,
        /// Target journal.
        #[arg(long)]
        to: Option<String>,
        /// Create a new journal with this name.
        #[arg(long, value_name = "NAME")]
        new: Option<String>,
        /// Project for the new journal.
        #[arg(long, requires = "new")]
        project: Option<String>,
    },
    /// Add a note to a journal, an entry, or selected words of an entry.
    #[command(group(ArgGroup::new("target").required(true).args(["journal", "entry"])))]
    Note {
        /// The note text.
        body: String,
        /// Note on the whole journal.
        #[arg(long, conflicts_with = "entry")]
        journal: Option<String>,
        /// Note on an entry.
        #[arg(long)]
        entry: Option<String>,
        #[command(flatten)]
        select: SelectArgs,
    },
    /// Highlight selected words of an entry.
    #[command(group(ArgGroup::new("words").required(true).args(["on", "range"])))]
    Highlight {
        entry: String,
        #[command(flatten)]
        select: SelectArgs,
        /// Color name, e.g. yellow.
        #[arg(long)]
        color: Option<String>,
        /// A comment kept with the highlight.
        #[arg(long, default_value = "")]
        comment: String,
    },
    /// Change a note's text (or a highlight's comment).
    EditNote { id: String, body: String },
    /// Remove a note or highlight.
    #[command(alias = "unmark")]
    RemoveNote { id: String },
    /// Attach an image to an entry (stored in the database; a reference is added to the text).
    Attach {
        entry: String,
        file: PathBuf,
        /// Name to store (default: the file name).
        #[arg(long)]
        name: Option<String>,
    },
    /// Write an attached image to a file.
    Image {
        id: String,
        #[arg(long)]
        out: PathBuf,
    },
}

/// Where the master key comes from.
#[derive(Debug, Clone, Args, Default)]
pub struct MasterInput {
    /// The 24 recovery words (prefer --words-file).
    #[arg(long)]
    pub words: Option<String>,
    /// File holding the 24 words (plain text or .age).
    #[arg(long)]
    pub words_file: Option<PathBuf>,
    /// Shard file (plain text or .age); give at least 3.
    #[arg(long = "shard")]
    pub shards: Vec<PathBuf>,
    /// Exported master key file (.age).
    #[arg(long)]
    pub master_file: Option<PathBuf>,
}

impl MasterInput {
    pub fn is_empty(&self) -> bool {
        self.words.is_none() && self.words_file.is_none() && self.shards.is_empty() && self.master_file.is_none()
    }
}

#[derive(Debug, Subcommand)]
pub enum KeysCommand {
    /// Create the master key, recovery words, shards and an encrypted database.
    Init {
        /// Machine-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Show key status.
    Status,
    /// Rotate the service key (needs the master key).
    Rotate {
        #[command(flatten)]
        master: MasterInput,
    },
    /// Re-encrypt the database with a fresh data key (needs the master key).
    Rekey {
        #[command(flatten)]
        master: MasterInput,
    },
    /// Export key material as separate password-encrypted files.
    Export {
        #[arg(long)]
        out: PathBuf,
        /// Which items: master, words, service, shards (default all).
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,
        #[command(flatten)]
        master: MasterInput,
    },
    /// Recover access from words, shards or the master key file.
    Recover {
        #[command(flatten)]
        master: MasterInput,
        /// Also print the 24 words.
        #[arg(long)]
        print_words: bool,
    },
    /// Re-issue the 3-of-5 dark shards (new incompatible set).
    Shards {
        #[arg(long)]
        out: Option<PathBuf>,
        #[command(flatten)]
        master: MasterInput,
    },
}

#[derive(Debug, Subcommand)]
pub enum BackupCommand {
    /// Create a backup archive in DIR.
    Create {
        #[arg(long)]
        out: PathBuf,
        /// Also encrypt the archive with the password (age).
        #[arg(long)]
        encrypt: bool,
    },
    /// Restore from a backup archive (or the newest one in a directory).
    Restore {
        #[arg(long)]
        from: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub enum ThemeCommand {
    List,
    /// Show the active theme and resolved mode.
    Show,
    Set {
        name: String,
    },
    Mode {
        #[arg(value_enum)]
        mode: ThemeMode,
    },
    Export {
        name: String,
        #[arg(long)]
        out: PathBuf,
    },
    Import {
        file: PathBuf,
    },
    /// Restore a built-in theme to its shipped version.
    Reset {
        name: String,
    },
}

/// Extra text for `mnnn tui --help` and mnnn-tui(1); details in docs/tui.md and docs/import.md.
const TUI_NOTES: &str = "\
Everything you can act on is clickable: menus, tiles, sidebar, popup rows, form fields, \
import picker entries and the key buttons ([X] closes any popup).

Import (i): pick a file or a whole folder (A imports the folder, recursing). If a symbol \
cannot be guessed from a file name, a popup asks for it, showing the file name on its own \
line, its folder below and (file i of n) during folder imports. After an import, if no tile shows \
data yet, the focused tile loads the first imported series.

Help (? or F1): follow links with Tab and Enter or a click, Backspace goes back, / searches \
from any topic; a clickable key bar at the bottom lists the shortcuts.

Tiles: Tab / Shift-Tab focus the next / previous tile, Alt-1..9 jumps to tile [n]; the \
status line shows these keys.

Ranges: [ and ] (or a click on a date) open a date and time picker for a chart's \
period or a list tile's range (from / to; \\ clears it); arrows or digits set year, month, \
day, hour and minute. T (Data > Filters > Time range...) sets one time range for every view and \
tile, from then to; x leaves an end open.

Symbols: the sidebar symbol filter (m, Data > Filters > Symbol) lists only the chosen \
symbols; search ignores separators (btc-usdt finds BTC/USDT) and word order.

Loading: series load in the background; a tile shows a progress bar (bars read of the total) \
or a spinner while the total is unknown, and the UI stays responsive.

History (Y, View > History): a collapsible bar on the left shows every action as a tree, \
forks indented; u / U step back / forward and restore the filters, time range and theme, \
Enter or a click on a row goes to that action. Authors (A, File > Authors...): add, edit, \
delete and pick the current author that new actions are attributed to.

See mnnn help tui, mnnn help import, mnnn help history, mnnn help authors and mnnn help help.";

/// Extra text for `mnnn help --help` and mnnn-help(1).
const HELP_NOTES: &str = "\
In the TUI help (? or F1): Tab / Shift-Tab select a link, Enter or a click follows it, \
Backspace or Alt-Left goes back, / searches from any topic, i opens the index, Esc closes. \
A key bar at the bottom shows these keys and every button is clickable.";

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        let mut v = vec!["mnnn"];
        v.extend_from_slice(args);
        Cli::try_parse_from(v)
    }

    #[test]
    fn author_and_history_commands() {
        match parse(&["author", "add", "--name", "Ada", "--email", "a@x.io", "--detail", "org=X", "--current"]).unwrap().command {
            Command::Author(AuthorCommand::Add { name, email, details, current }) => {
                assert_eq!((name.as_str(), email.as_deref(), details.len(), current), ("Ada", Some("a@x.io"), 1, true));
            }
            c => panic!("{c:?}"),
        }
        assert!(matches!(parse(&["authors", "list"]).unwrap().command, Command::Author(AuthorCommand::List { json: false })));
        assert!(matches!(parse(&["author", "use", "--none"]).unwrap().command, Command::Author(AuthorCommand::Use { author: None, none: true })));
        assert!(parse(&["author", "use"]).is_err());
        assert!(matches!(parse(&["history", "back"]).unwrap().command, Command::History(HistoryCommand::Back { steps: 1 })));
        assert!(matches!(parse(&["history", "list", "-n", "5"]).unwrap().command, Command::History(HistoryCommand::List { limit: 5, .. })));
        assert!(matches!(parse(&["history", "goto", "0"]).unwrap().command, Command::History(HistoryCommand::Goto { id: 0 })));
    }

    #[test]
    fn search_project_journal_commands() {
        match parse(&["search", "save", "btc", "--query", "btc", "--source", "binance", "--year", "2024", "--desc"]).unwrap().command {
            Command::Search(SearchCommand::Save { name, spec }) => {
                assert_eq!(
                    (name.as_str(), spec.query.as_deref(), spec.sources.len(), spec.years.as_slice(), spec.desc),
                    ("btc", Some("btc"), 1, &[2024][..], true)
                );
            }
            c => panic!("{c:?}"),
        }
        assert!(matches!(parse(&["search", "optimize", "x", "--clear"]).unwrap().command, Command::Search(SearchCommand::Edit { .. })));
        assert!(parse(&["search", "save", "x", "--desc", "--asc"]).is_err());
        assert!(parse(&["project", "add", "p"]).is_err(), "needs an item");
        assert!(matches!(parse(&["projects", "new", "P"]).unwrap().command, Command::Project(ProjectCommand::Create { .. })));
        assert!(parse(&["journal", "copy", "e1"]).is_err(), "needs --to or --new");
        assert!(parse(&["journal", "note", "hi"]).is_err(), "needs --journal or --entry");
        assert!(parse(&["journal", "highlight", "e"]).is_err(), "needs words");
        match parse(&["journal", "highlight", "e", "--on", "broke out", "--nth", "2"]).unwrap().command {
            Command::Journal(JournalCommand::Highlight { select, .. }) => assert_eq!((select.on.as_deref(), select.nth), (Some("broke out"), 2)),
            c => panic!("{c:?}"),
        }
        assert!(matches!(
            parse(&["journal", "new", "J", "--format", "latex"]).unwrap().command,
            Command::Journal(JournalCommand::New { format: FormatArg::Latex, .. })
        ));
    }

    #[test]
    fn cli_debug_assert() {
        Cli::command().debug_assert();
    }

    #[test]
    fn csv_defaults() {
        let cli = parse(&["ingest", "csv", "--path", "x.csv"]).unwrap();
        match cli.command {
            Command::Ingest(IngestCommand::Csv(a)) => {
                assert_eq!(a.delimiter, ',');
                assert_eq!(a.quote, '"');
                assert!(!a.no_header);
                assert_eq!(a.skip_rows, 0);
                assert!(a.escape.is_none());
            }
            _ => panic!(),
        }
    }

    #[test]
    fn ingest_path_positional_or_flag() {
        assert!(parse(&["ingest", "path", "/data"]).is_ok());
        assert!(parse(&["ingest", "path", "--path", "/data", "--no-recursive"]).is_ok());
        assert!(parse(&["ingest", "path"]).is_err());
        assert!(parse(&["ingest", "path", "/a", "--path", "/b"]).is_err());
    }

    #[test]
    fn quiet_conflicts_with_verbose() {
        assert!(parse(&["-q", "-v", "tui"]).is_err());
        assert!(parse(&["-q", "-y", "tui"]).is_ok());
    }

    #[test]
    fn redis_requires_key_or_pattern() {
        assert!(parse(&["ingest", "service", "redis", "--uri", "localhost"]).is_err());
        assert!(parse(&["ingest", "service", "redis", "--uri", "localhost", "--key", "a", "--pattern", "b"]).is_err());
        assert!(parse(&["ingest", "service", "redis", "--uri", "localhost", "--key", "a"]).is_ok());
    }

    #[test]
    fn postgres_url_conflicts() {
        assert!(parse(&["ingest", "service", "postgres", "--table", "t"]).is_err());
        assert!(parse(&["ingest", "service", "postgres", "--url", "postgres://u@h/db", "--table", "t"]).is_ok());
        assert!(parse(&["ingest", "service", "postgres", "--url", "postgres://u@h/db", "--host", "h", "--table", "t"]).is_err());
        assert!(parse(&["ingest", "service", "postgres", "--username", "u", "--host", "h", "--query", "select 1"]).is_ok());
        assert!(parse(&["ingest", "service", "postgres", "--username", "u", "--host", "h"]).is_err());
    }

    #[test]
    fn sqlite_table_or_query() {
        assert!(parse(&["ingest", "service", "sqlite", "--path", "a.db"]).is_err());
        assert!(parse(&["ingest", "service", "sqlite", "--path", "a.db", "--table", "bars"]).is_ok());
    }

    #[test]
    fn http_defaults() {
        let cli = parse(&["ingest", "service", "http", "--url", "http://x", "--param", "a=1", "-H", "X-A: b"]).unwrap();
        match cli.command {
            Command::Ingest(IngestCommand::Service(ServiceCommand::Http(h))) => {
                assert_eq!(h.method, HttpMethod::Get);
                assert_eq!(h.retries, 3);
                assert_eq!(h.common.timeout, 30);
                assert_eq!(h.params, vec!["a=1"]);
            }
            _ => panic!(),
        }
        assert!(parse(&["ingest", "service", "http", "--url", "http://x", "-X", "POST", "--body", "{}"]).is_ok());
    }
}
