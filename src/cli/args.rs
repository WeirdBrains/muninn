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
    /// Reports for people: text, records, series, journals, projects and authors, exported
    /// as PDF, Word, HTML, LaTeX, JSON or CSV.
    #[command(subcommand, alias = "reports")]
    Report(ReportCommand),
    /// Rules: if this action happens (and these fields match), then do that. They live on
    /// their own, in a journal, a live search or projects.
    #[command(subcommand, alias = "rules", after_long_help = RULE_NOTES)]
    Rule(RuleCommand),
    /// Models: record tables you design (typed fields, references, constraints), changed by
    /// versioned migrations you preview, apply and roll back; and their records.
    #[command(subcommand, alias = "models", after_long_help = MODEL_NOTES)]
    Model(ModelCommand),
    /// Code spaces: math, Polars, Rust or Python (or a prebuilt plugin) compiled into a native
    /// plugin that reads and writes your data and shows a result.
    #[command(subcommand, alias = "codes", after_long_help = CODE_NOTES)]
    Code(CodeCommand),
    /// The network: run a node, exchange keys with other nodes, share data with them, search
    /// across them and entrust them with dark shards. On with the TUI by default; nothing is shared until you ask.
    #[command(subcommand, alias = "network", after_long_help = NET_NOTES)]
    Net(NetCommand),
    /// Organizations: members with roles, shared resources and projects, rules and code
    /// answering requests, and an HTTPS site (HTML, JSON, gRPC) for what you publish.
    #[command(subcommand, alias = "orgs", after_long_help = ORG_NOTES)]
    Org(OrgCommand),
    /// Reviews: ask peers to view, comment on, suggest changes to, highlight and vote on an
    /// entry, journal, project, live search or record; accept, reject and merge what they say.
    #[command(subcommand, alias = "reviews", after_long_help = REVIEW_NOTES)]
    Review(ReviewCommand),
    /// Share anything shareable to anything that takes shares: a project, a journal (a new
    /// entry about it), a code space (a model as its input) or your exchanges (`net`).
    Share(ShareArgs),
    /// You as a user: your identity across your nodes, linking another node to it and
    /// revoking one.
    #[command(subcommand, alias = "users", after_long_help = USER_NOTES)]
    User(UserCommand),
    /// Connections: the users your nodes know, trusted or untrusted (default untrusted).
    #[command(subcommand, alias = "connections", after_long_help = CONNECTION_NOTES)]
    Connection(ConnectionCommand),
    /// Invites: links that connect, bring someone into an org or a review; list, revoke and
    /// accept them.
    #[command(subcommand, alias = "invites", after_long_help = INVITE_NOTES)]
    Invite(InviteCommand),
    /// Sync: every node linked to your user keeps all of your data; conflicts are kept, never
    /// lost; bars on request.
    #[command(subcommand, after_long_help = SYNC_NOTES)]
    Sync(SyncCommand),
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
    /// A document: a summary and every bar per series.
    Html,
    Latex,
    Pdf,
    Docx,
    Text,
}

/// Document formats (`archive`: projects only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DocFormatArg {
    Pdf,
    Html,
    Docx,
    Latex,
    Json,
    Csv,
    Text,
    Archive,
}

impl From<DocFormatArg> for crate::export::files::OutFormat {
    fn from(f: DocFormatArg) -> Self {
        use crate::export::files::OutFormat as O;
        match f {
            DocFormatArg::Pdf => O::Pdf,
            DocFormatArg::Html => O::Html,
            DocFormatArg::Docx => O::Docx,
            DocFormatArg::Latex => O::Latex,
            DocFormatArg::Json => O::Json,
            DocFormatArg::Csv => O::Csv,
            DocFormatArg::Text => O::Text,
            DocFormatArg::Archive => O::Archive,
        }
    }
}

#[derive(Debug, Clone, Args)]
pub struct ExportArgs {
    #[arg(long, value_enum)]
    pub format: ExportFormat,
    /// Output path, or `-` for stdout (csv/json/text).
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
    #[arg(long, conflicts_with_all = ["series", "records"])]
    pub journals: bool,
    /// Search series (the default).
    #[arg(long, conflicts_with = "records")]
    pub series: bool,
    /// Search the records of designed models (every model but bars, or --model).
    #[arg(long)]
    pub records: bool,
    /// Only this model's records (implies --records; `all` searches every model again).
    #[arg(long)]
    pub model: Option<String>,
    /// Records meeting a condition, FIELD OP VALUE (implies --records). Repeatable.
    #[arg(long = "where", value_name = "COND")]
    pub conditions: Vec<String>,
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
        /// Also run it on every peer that shares with you (results say where they came from).
        #[arg(long)]
        network: bool,
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
    /// Add a live search, a journal, a report, a rule, a model, a code space or a result (a
    /// series, optionally a time range) to a project.
    #[command(group(ArgGroup::new("item").required(true).args(["search", "journal", "report", "rule", "model", "code", "result"])))]
    Add {
        project: String,
        /// Live search id or name.
        #[arg(long)]
        search: Option<String>,
        /// Rule id or name.
        #[arg(long)]
        rule: Option<String>,
        /// Journal id or name.
        #[arg(long)]
        journal: Option<String>,
        /// Report id or name.
        #[arg(long)]
        report: Option<String>,
        /// Model id or name.
        #[arg(long)]
        model: Option<String>,
        /// Code space id or name.
        #[arg(long)]
        code: Option<String>,
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
    /// Export a project: one archive with everything (journals, images, reports, live
    /// searches, results and their bars), or a document of it.
    Export {
        project: String,
        #[arg(long, value_enum, default_value = "archive")]
        format: DocFormatArg,
        /// Output file (default: <project>.<ext> in the current directory).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Bars listed per result in documents (0: all).
        #[arg(long, default_value_t = 20)]
        rows: usize,
        /// Leave the bars out of archives and JSON.
        #[arg(long)]
        no_data: bool,
    },
    /// Import a project archive (.mnnp) or JSON pack as a new project.
    Import {
        file: PathBuf,
        /// Name for the imported project (default: its name, "(imported)" added if taken).
        #[arg(long)]
        name: Option<String>,
    },
}

/// What a new report section shows (choose one).
#[derive(Debug, Clone, Args)]
#[command(group(ArgGroup::new("kind").required(true).args(["text", "file", "records", "series", "journal", "project", "authors", "code", "model"])))]
pub struct SectionArgs {
    pub report: String,
    /// Section title (default: one from what it shows).
    #[arg(long)]
    pub title: Option<String>,
    #[command(flatten)]
    pub body: TextArgs,
    /// Format of the text.
    #[arg(long, value_enum)]
    pub format: Option<FormatArg>,
    /// Records of a series as SOURCE:SYMBOL:PERIOD: a summary and a table of bars.
    #[arg(long, value_name = "SERIES")]
    pub records: Option<String>,
    /// Records time range start.
    #[arg(long, value_name = "TIME", requires = "records")]
    pub from: Option<String>,
    /// Records time range end.
    #[arg(long, value_name = "TIME", requires = "records")]
    pub to: Option<String>,
    /// Bars (the latest) or model records listed (0: all). Default 20.
    #[arg(long)]
    pub rows: Option<usize>,
    /// Records of a designed model (`mnnn model list`), with its fields.
    #[arg(long, value_name = "MODEL")]
    pub model: Option<String>,
    /// Only --model records meeting a condition (`price > 3`). Repeatable.
    #[arg(long = "where", value_name = "COND", requires = "model")]
    pub conditions: Vec<String>,
    /// A table of series: what a live search matches now (id or name), or every series.
    #[arg(long, value_name = "SEARCH", num_args = 0..=1, default_missing_value = "")]
    pub series: Option<String>,
    /// A journal's entries (with notes and highlights).
    #[arg(long)]
    pub journal: Option<String>,
    /// Only these entries of --journal. Repeatable.
    #[arg(long = "entry", requires = "journal")]
    pub entries: Vec<String>,
    /// A project's contents.
    #[arg(long)]
    pub project: Option<String>,
    /// Everyone who contributed to the report.
    #[arg(long)]
    pub authors: bool,
    /// A code space (name or id): its last result and its source.
    #[arg(long, value_name = "SPACE")]
    pub code: Option<String>,
    /// Put the section at this position (1 = first; default: last).
    #[arg(long)]
    pub at: Option<usize>,
}

pub const RULE_NOTES: &str = "\
Conditions (--if, all must hold): FIELD OP VALUE with = != ~ (contains) !~ > >= < <=,
or FIELD ? (present) / FIELD !? (missing). Fields: kind, summary, origin, author,
action, before.X, after.X and the action's params (see `mnnn rule events`). = takes *
wildcards; numbers compare as numbers; text ignores case.

Then-actions (--then, run in order):
  status TEXT                               message (TUI status line, stderr)
  search [SEARCH]                           run a live search, report its matches
  export-report REPORT to=PATH [format=F]   export a report
  export-journal [JOURNAL] to=PATH [format=F]
  add-to-project [PROJECT] [item=subject|search:S|journal:J|report:R]
  entry [JOURNAL] \"TEXT\" [title=T]          write a journal entry
  note [JOURNAL] \"TEXT\"                     add a note to a journal
  model-create NAME fields=\"a:text; b:real\" [about=T] [draft=yes]
  model-edit [MODEL] add=SPEC change=SPEC drop=F rename=A:B move=F:N [apply=yes] [destructive=yes]
  model-apply [MODEL] [destructive=yes]     apply a model's draft
  model-remove [MODEL] [destructive=yes]
  record-create [MODEL] FIELD=VALUE…        add a record (values take {FIELD})
  record-update [MODEL] id=ID|where=\"F OP V\" FIELD=VALUE…
  record-delete [MODEL] id=ID|where=\"F OP V\"
Left out, a target is the rule's own journal, live search, model or project. Texts and paths
take {kind} {summary} {origin} {author} {action} {rule} {date} {time} {now} and {FIELD}.

Examples:
  mnnn rule create \"Weekly after import\" --on import --if \"inserted > 0\" \\
      --then \"export-report Weekly to=~/reports/weekly-{date}.pdf\"
  mnnn rule create Collect --project Halving --on search.save --then add-to-project
  mnnn rule test Collect            # against the newest action
  mnnn rule dry-run Collect --action 42";

#[derive(Debug, Args)]
pub struct RuleSpecArgs {
    /// Action kinds to react to: import, journal.*, * (repeat or comma-separate).
    #[arg(long = "on", value_name = "KIND")]
    pub on: Vec<String>,
    /// A condition: FIELD OP VALUE, e.g. "inserted > 0" (repeat: all must hold).
    #[arg(long = "if", value_name = "CONDITION")]
    pub when: Vec<String>,
    /// A then-action, e.g. "status Imported {inserted} bars" (repeat: run in order).
    #[arg(long = "then", value_name = "ACTION")]
    pub then: Vec<String>,
}

#[derive(Debug, Subcommand)]
pub enum RuleCommand {
    /// Create a rule.
    #[command(alias = "new")]
    Create {
        name: String,
        #[command(flatten)]
        spec: RuleSpecArgs,
        /// Put it in a project (as a project item).
        #[arg(long, conflicts_with_all = ["journal", "search"])]
        project: Option<String>,
        /// Let it live in a journal (its default journal).
        #[arg(long, conflicts_with = "search")]
        journal: Option<String>,
        /// Let it live in a live search (its default search).
        #[arg(long, conflicts_with = "model")]
        search: Option<String>,
        /// Let it live in a model (its default model for record-… and model-… actions).
        #[arg(long)]
        model: Option<String>,
        /// Only react to actions about where it lives.
        #[arg(long)]
        related: bool,
        /// Create it switched off.
        #[arg(long)]
        disabled: bool,
        #[arg(long, default_value = "")]
        description: String,
    },
    /// Change a rule: given --on, --if or --then replace the old ones.
    Edit {
        rule: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[command(flatten)]
        spec: RuleSpecArgs,
        /// Drop every condition.
        #[arg(long, conflicts_with = "when")]
        clear_if: bool,
        /// Move it into a journal.
        #[arg(long, conflicts_with_all = ["search", "standalone"])]
        journal: Option<String>,
        /// Move it into a live search.
        #[arg(long, conflicts_with_all = ["standalone", "model"])]
        search: Option<String>,
        /// Move it into a model.
        #[arg(long, conflicts_with = "standalone")]
        model: Option<String>,
        /// Take it out of its journal or live search (projects are kept).
        #[arg(long)]
        standalone: bool,
        /// Only react to actions about where it lives.
        #[arg(long, conflicts_with = "unrelated")]
        related: bool,
        /// React to every matching action.
        #[arg(long)]
        unrelated: bool,
    },
    /// List rules.
    #[command(alias = "ls")]
    List {
        /// Only the rules in this project.
        #[arg(long)]
        project: Option<String>,
        /// Only the rules living in this journal.
        #[arg(long)]
        journal: Option<String>,
        /// Only the rules living in this live search.
        #[arg(long)]
        search: Option<String>,
        /// Only the rules living in this model.
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Show a rule and its recent runs.
    Show {
        rule: String,
        #[arg(long)]
        json: bool,
    },
    /// Delete a rule (it leaves its projects).
    #[command(alias = "rm")]
    Remove { rule: String },
    /// Switch a rule on.
    Enable { rule: String },
    /// Switch a rule off.
    Disable { rule: String },
    /// Check a rule (or every rule) against an action: which parts match and why.
    Test {
        rule: Option<String>,
        /// Action number (default: the newest, see `mnnn history list`).
        #[arg(long)]
        action: Option<i64>,
        #[arg(long)]
        json: bool,
    },
    /// What a rule would do for an action, without doing it.
    #[command(name = "dry-run")]
    DryRun {
        rule: Option<String>,
        /// Action number (default: the newest).
        #[arg(long)]
        action: Option<i64>,
        #[arg(long)]
        json: bool,
    },
    /// The action kinds rules react to, with their fields.
    Events {
        /// Only kinds matching this (e.g. journal.*).
        kind: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum ReportCommand {
    /// List reports.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Create a report (optionally in a project).
    #[command(alias = "new")]
    Create {
        name: String,
        #[arg(long, default_value = "")]
        description: String,
        /// Add it to this project.
        #[arg(long)]
        project: Option<String>,
    },
    /// Show a report's sections (--json: the definition).
    Show {
        report: String,
        #[arg(long)]
        json: bool,
    },
    /// Rename a report or change its description.
    Edit {
        report: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
    },
    /// Remove a report (and its place in projects).
    #[command(alias = "rm")]
    Remove { report: String },
    /// Add a section: text, records, series, a journal, a project, authors or code output.
    Add(SectionArgs),
    /// Change a section's title, text, format or rows.
    EditSection {
        report: String,
        /// Section number (from `mnnn report show`) or id.
        section: String,
        #[arg(long)]
        title: Option<String>,
        #[command(flatten)]
        body: TextArgs,
        #[arg(long, value_enum)]
        format: Option<FormatArg>,
        /// Bars listed by a records section (0: all).
        #[arg(long)]
        rows: Option<usize>,
    },
    /// Remove a section.
    RemoveSection { report: String, section: String },
    /// Move a section to another position.
    Move {
        report: String,
        section: String,
        /// New position (1 = first).
        #[arg(long)]
        to: usize,
    },
    /// Export a report with the data as it is now.
    Export {
        report: String,
        #[arg(long, value_enum, default_value = "pdf")]
        format: DocFormatArg,
        /// Output file (default: <report>.<ext> in the current directory).
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Print a report as text in the terminal.
    Preview {
        report: String,
        #[arg(long, default_value_t = 100)]
        width: usize,
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
    /// Export a journal (or some entries) as PDF, Word, HTML, LaTeX, text or JSON (JSON can
    /// be imported again).
    Export {
        journal: String,
        /// Only these entries. Repeatable.
        #[arg(long = "entry")]
        entries: Vec<String>,
        #[arg(long, value_enum, default_value = "pdf")]
        format: DocFormatArg,
        /// Output file (default: <name>.<ext> in the current directory).
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Import a journal exported as JSON (as a new journal).
    Import {
        file: PathBuf,
        /// Put it in this project.
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        name: Option<String>,
    },
    /// Print a journal (or some entries): a PDF sent to lp/lpr, or saved if there is none.
    Print {
        journal: String,
        /// Only these entries. Repeatable.
        #[arg(long = "entry")]
        entries: Vec<String>,
        /// Printer name (default: the system default).
        #[arg(long)]
        printer: Option<String>,
        /// Where to keep the PDF (default: ~/.muninn/prints/).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Only make and keep the PDF; do not send it to a printer.
        #[arg(long, conflicts_with = "printer")]
        save_only: bool,
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

Projects menu (Alt-P): Projects... (P) lists projects and their live searches, results and \
journals; Live searches... (F) and Save search as live search... (B) keep a sidebar search \
that always runs on current data; Journals... (J) holds entries in Markdown or LaTeX with \
images, notes and highlights on selected words (v selects, m highlights, n notes); E exports \
and P prints a journal, the marked entries or an entry. Reports... (O) builds reports from \
text, records, series, journals, projects and authors (a adds a section, J/K move it, v \
previews). E exports journals, reports and projects (PDF, HTML, Word, LaTeX, JSON, CSV, text, \
project archives); I in the projects list imports one. In the sidebar, a adds the selected series to a project.

See mnnn help tui, mnnn help import, mnnn help history, mnnn help authors, \
mnnn help projects, mnnn help live-searches, mnnn help journals, mnnn help reports, \
mnnn help export-formats and mnnn help help.";

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
    fn code_commands() {
        match parse(&[
            "code",
            "create",
            "f",
            "--lang",
            "python",
            "--model",
            "bars",
            "--where",
            "a = 1",
            "--where",
            "b > 2",
            "--sort",
            "-timestamp",
            "--limit",
            "3",
        ])
        .unwrap()
        .command
        {
            Command::Code(CodeCommand::Create { name, lang, input, source, .. }) => {
                assert_eq!((name.as_str(), lang), ("f", CodeLang::Python));
                assert_eq!(input.filter, vec!["a = 1", "b > 2"]);
                assert_eq!(input.sort, vec!["-timestamp"]);
                assert_eq!(input.limit, Some(3));
                assert!(source.file.is_none() && !source.editor);
            }
            c => panic!("{c:?}"),
        }
        assert!(matches!(parse(&["codes", "run", "f", "--json"]).unwrap().command, Command::Code(CodeCommand::Run { json: true, rows: 30, .. })));
        assert!(matches!(parse(&["code", "check"]).unwrap().command, Command::Code(CodeCommand::Check { space: None })));
        assert!(parse(&["code", "create", "f", "--lang", "cobol"]).is_err());
        assert!(parse(&["code", "create", "f", "--lang", "math", "--file", "a", "--source", "b"]).is_err(), "one source");
        assert!(parse(&["code", "edit", "f", "--no-input", "--model", "bars"]).is_err());
        assert!(matches!(
            parse(&["project", "add", "P", "--code", "f"]).unwrap().command,
            Command::Project(ProjectCommand::Add { code: Some(_), .. })
        ));
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

pub const NET_NOTES: &str = "\
The node starts with the TUI (turn that off with `mnnn net config --autostart off`) or with
`mnnn net start`; other commands bring up a short-lived node with the same identity when they
need to talk. Nothing is shared until you `share` it, and only with nodes you are connected to.

Connecting exchanges keys automatically. A node that knows you (or that you both asked)
connects at once; otherwise its owner sees a request to accept or decline. Keys are pinned to
the node on first contact, and you are warned if another node uses a known name. The token and
short-code commands are still there, under `mnnn net exchange` (advanced).

Examples:
  mnnn net start --for 3600
  mnnn net connect /ip4/192.0.2.7/tcp/4741/p2p/12D3Koo… --label ben
  mnnn net invite                       # a link; they run `mnnn net connect LINK`
  mnnn net requests
  mnnn net accept ben
  mnnn net share journal \"field notes\" --with ben
  mnnn net search --journals --query comet
  mnnn net fetch ben \"field notes\" --import
  mnnn net shard entrust --to ben --to cleo --to dev --threshold 2
  mnnn net shard restore --backup net-backup.age --recover

See `mnnn help network`, `mnnn help key-exchanges` and `mnnn help dark-shards`.";

pub const USER_NOTES: &str = "\
Your user identity is a signing key of its own, made on first use and named after the current
author. It certifies each node that acts for you; peers check those certificates, so a phone or
a second computer is recognised as you. Linking: on the new node run `mnnn user link TARGET`
(an address, peer or invite link of one of your nodes), then approve it there with
`mnnn user approve`. A revoked node stays revoked; peers learn it the next time they talk.
See `mnnn help users`.

Examples:
  mnnn user show
  mnnn user label \"Ada Lovelace\"
  mnnn user link muninn://join/…
  mnnn user approve phone
  mnnn user nodes
  mnnn user revoke phone
";

pub const CONNECTION_NOTES: &str = "\
Connecting to a node (an invite link or `mnnn net connect`) also tells both sides whom the
other node acts for; that user becomes a connection, untrusted until you trust it. Trust
changes are kept in history. Dark shards only go to trusted connections.
See `mnnn help users`.

Examples:
  mnnn connection list
  mnnn connection trust ben
  mnnn connection trust ben --off
  mnnn connection remove ben
";

pub const INVITE_NOTES: &str = "\
An invite is a signed, expiring link (`muninn://join/…`) that holds no secret. `create` makes one
to connect (default), to join an org (--org) or to review (--review). The other side uses it
with `mnnn invite accept LINK`. `list` shows the links you made and the requests waiting for
you; `revoke` withdraws an unused one.
See `mnnn help users`.

Examples:
  mnnn invite create --label ben
  mnnn invite create --org research --role viewer
  mnnn invite create --review draft-1
  mnnn invite list
  mnnn invite revoke 3f2a
  mnnn invite accept muninn://join/…
";

#[derive(Debug, Subcommand)]
pub enum UserCommand {
    /// Your user: id, name and linked nodes.
    Show {
        #[arg(long)]
        json: bool,
    },
    /// The users this node knows (you, connections, org members, reviewers).
    List {
        #[arg(long)]
        json: bool,
    },
    /// Rename your user (peers see it the next time they talk).
    Label { name: String },
    /// The nodes linked to a user (default: you), revoked ones too.
    Nodes {
        user: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Link this node to the user of another node of yours (address, peer, name or invite
    /// link); that node's owner approves it.
    Link { target: String },
    /// Approve a node asking to link to you (default: the only request).
    Approve { request: Option<String> },
    /// Revoke one of your nodes (peer id, its end, or label); it no longer acts for you.
    Revoke { node: String },
}

#[derive(Debug, Subcommand)]
pub enum ConnectionCommand {
    /// Your connections, with trust.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Trust a connection (or stop trusting it with --off).
    Trust {
        user: String,
        #[arg(long)]
        off: bool,
    },
    /// Forget a connection (the key exchange stays; trust is reset).
    Remove { user: String },
}

pub const SYNC_NOTES: &str = "\
A running node pulls what changed from each of your linked nodes about once a minute, over
the key exchange and sealed with a key only your nodes hold. Rows changed on both nodes
keep the newer one; the other is saved as a conflict (`mnnn sync conflicts`) and can be put
back with `mnnn sync resolve ID --take-other`. A delete never wins over an edit. Bars are
not synced on their own: `mnnn sync bars` fetches them. Revoked nodes are not answered.
See `mnnn help sync`.

Examples:
  mnnn sync status
  mnnn sync now
  mnnn sync conflicts
  mnnn sync show 3f2a
  mnnn sync resolve 3f2a --take-other
  mnnn sync bars --source binance --symbol BTCUSDT
";

#[derive(Debug, Subcommand)]
pub enum SyncCommand {
    /// Your linked nodes: when each last synced, what it brought and open conflicts.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Pull from your linked nodes now (all, or one by label or peer id).
    Now { node: Option<String> },
    /// Rows changed on two nodes at once: which one was kept, and the other.
    Conflicts {
        /// Resolved ones too.
        #[arg(long)]
        all: bool,
        #[arg(long)]
        json: bool,
    },
    /// Both sides of one conflict.
    Show { id: String },
    /// Settle a conflict: keep what is there, or put the other row back (--take-other).
    Resolve {
        id: String,
        #[arg(long)]
        take_other: bool,
    },
    /// Fetch bars from a linked node (all series, or some).
    Bars {
        /// A linked node (label or peer id; default: all).
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        symbol: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum InviteCommand {
    /// Make an invite link: to connect (default), to an org (--org) or a review (--review).
    Create {
        /// Call whoever uses it this.
        #[arg(long, default_value = "")]
        label: String,
        #[arg(long, conflicts_with = "review")]
        org: Option<String>,
        /// The role for an org invite (default: the org's invite role).
        #[arg(long, requires = "org")]
        role: Option<String>,
        #[arg(long)]
        review: Option<String>,
        /// Send it straight to a known peer instead of printing it.
        #[arg(long)]
        peer: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// The invites you made and the requests waiting for you.
    List {
        /// Expired, used and revoked ones too.
        #[arg(long)]
        all: bool,
        #[arg(long)]
        json: bool,
    },
    /// Withdraw an invite you made (id or its start).
    Revoke { invite: String },
    /// Use an invite link, or accept a request (id or name).
    Accept { invite: String },
    /// Turn down a request.
    Decline { request: String },
}

/// Network commands.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Subcommand)]
pub enum NetCommand {
    /// Run this node in the foreground until Ctrl-C (or --for SECS, or `mnnn net stop`).
    Start {
        /// TCP and QUIC port (default from settings: 4741; 0 picks one).
        #[arg(long)]
        port: Option<u16>,
        /// Listen on these addresses instead (multiaddr, repeatable).
        #[arg(long = "listen", value_name = "ADDR")]
        listen: Vec<String>,
        /// Do not look for peers on the local network.
        #[arg(long)]
        no_mdns: bool,
        /// Relay traffic for other nodes (and say so).
        #[arg(long)]
        relay_server: bool,
        /// Stop after this many seconds.
        #[arg(long = "for", value_name = "SECS")]
        for_secs: Option<u64>,
    },
    /// Ask a node started with `mnnn net start` to stop.
    Stop,
    /// This node: identity, addresses, connections, NAT, relays.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Peers seen or known.
    Peers {
        #[arg(long)]
        json: bool,
    },
    /// Connect to a node (`…/p2p/ID`, a peer id, a known name or an invite link): keys are
    /// exchanged automatically; a node that does not know you yet asks its owner first.
    Connect {
        target: String,
        /// Call it this.
        #[arg(long)]
        label: Option<String>,
        /// Only say hello and remember it (no key exchange).
        #[arg(long)]
        hello: bool,
    },
    /// Make an invite link for another node: when they use it (`mnnn net connect LINK`, or
    /// paste it in the TUI), the two nodes connect. It holds no secret and expires in 7 days.
    Invite {
        /// Call the node that uses it this.
        #[arg(long)]
        label: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Requests waiting for you: nodes that want to connect, apply or invite you.
    Requests {
        #[arg(long)]
        json: bool,
    },
    /// Accept a request (its id or the node's name).
    Accept { request: String },
    /// Decline a request (its id or the node's name).
    Decline { request: String },
    /// End the connection with a node: keys are erased on both sides.
    Disconnect { peer: String },
    /// Forget a known peer (exchanges with it are kept).
    Forget { peer: String },
    /// Relays.
    #[command(subcommand)]
    Relay(NetRelayCommand),
    /// Show or change network settings.
    Config {
        /// How this node introduces itself.
        #[arg(long)]
        label: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        /// Replace the listen addresses (repeatable; \"default\" for all interfaces).
        #[arg(long = "listen", value_name = "ADDR")]
        listen: Vec<String>,
        /// Look for peers on the local network.
        #[arg(long, value_name = "on|off")]
        mdns: Option<OnOff>,
        /// Start the node when the TUI opens (on by default).
        #[arg(long, value_name = "on|off")]
        autostart: Option<OnOff>,
        /// Connect on its own with `known` nodes only (default) or `anyone`.
        #[arg(long, value_name = "known|anyone")]
        accept: Option<String>,
        /// Add a bootstrap node (repeatable).
        #[arg(long = "bootstrap", value_name = "ADDR")]
        bootstrap: Vec<String>,
        /// Remove all bootstrap nodes first.
        #[arg(long)]
        clear_bootstrap: bool,
        /// Add an address other hosts reach this node at from outside: `host`, `host:port`
        /// or a multiaddr (repeatable; DNS names are fine). Goes first in invite links.
        #[arg(long = "external", value_name = "HOST[:PORT]")]
        external: Vec<String>,
        /// Remove all external addresses first.
        #[arg(long)]
        clear_external: bool,
        #[arg(long)]
        json: bool,
    },
    /// Advanced: key exchanges by hand (tokens, short codes). `connect` does this for you.
    #[command(subcommand, alias = "x", hide = true)]
    Exchange(NetExchangeCommand),
    /// Share an item: model, search, journal, project, report or code.
    Share {
        /// model, search, journal, project, report or code.
        kind: String,
        /// Id, id prefix or name.
        item: String,
        /// Only with these exchanges or peers (default: every exchange).
        #[arg(long = "with", value_name = "EXCHANGE")]
        with: Vec<String>,
    },
    /// Stop sharing an item (everywhere, or with --with).
    Unshare {
        kind: String,
        item: String,
        #[arg(long = "with", value_name = "EXCHANGE")]
        with: Vec<String>,
    },
    /// What this node shares, and with whom.
    Shared {
        #[arg(long)]
        json: bool,
    },
    /// What a peer shares with us (all peers with an exchange when none is given).
    List {
        exchange: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Fetch an item a peer shares.
    Fetch {
        /// Exchange or peer.
        exchange: String,
        /// Shared item: id, kind/name or name.
        item: String,
        /// json (default), md, html, latex, pdf, docx, csv, mnnp (projects).
        #[arg(long)]
        format: Option<String>,
        /// Write to this file (default: print, or a file named after the item for binary formats).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Add it to this database (journals, projects, code spaces, live searches).
        #[arg(long)]
        import: bool,
    },
    /// Search here and on every peer that shares with you; results say where they came from.
    Search {
        #[command(flatten)]
        spec: SpecArgs,
        /// A saved live search to run instead of the options.
        #[arg(long, value_name = "SEARCH")]
        saved: Option<String>,
        /// Only these exchanges or peers.
        #[arg(long = "only", value_name = "EXCHANGE")]
        only: Vec<String>,
        /// Leave out local results.
        #[arg(long)]
        remote_only: bool,
        /// At most this many results per peer.
        #[arg(long, default_value_t = 200)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Dark shards: the master key split among peers you exchanged keys with.
    #[command(subcommand)]
    Shard(NetShardCommand),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OnOff {
    On,
    Off,
}

impl OnOff {
    pub fn on(self) -> bool {
        self == OnOff::On
    }
}

#[derive(Debug, Subcommand)]
pub enum NetRelayCommand {
    /// Relays known (configured, and seen announcing it).
    List {
        #[arg(long)]
        json: bool,
    },
    /// Use a relay (`…/p2p/ID`): reachable through it when behind NAT.
    Add { addr: String },
    /// Stop using a relay.
    Remove { relay: String },
    /// Be a relay for others (on|off).
    Serve { state: OnOff },
}

#[derive(Debug, Subcommand)]
pub enum NetExchangeCommand {
    /// Offer a key exchange: prints a token to pass on, or sends it with --peer.
    Create {
        /// What to call this exchange.
        #[arg(long, default_value = "")]
        label: String,
        /// Send the offer to this node (`…/p2p/ID`, peer id or known peer).
        #[arg(long)]
        peer: Option<String>,
        /// Hours the offer stays valid.
        #[arg(long, default_value_t = 168)]
        hours: u64,
        #[arg(long)]
        json: bool,
    },
    /// Accept an offer (incoming exchange id or token), or complete ours with an answer token.
    Accept {
        /// Exchange id, `-` to read a token from stdin, or the token.
        input: String,
        #[arg(long, default_value = "")]
        label: String,
        #[arg(long)]
        json: bool,
    },
    /// Key exchanges.
    List {
        #[arg(long)]
        json: bool,
    },
    /// One exchange, with its short code and fingerprint.
    Show {
        exchange: String,
        /// Print the offer and answer tokens too.
        #[arg(long)]
        tokens: bool,
        #[arg(long)]
        json: bool,
    },
    /// Confirm the short code the other side read out.
    Verify { exchange: String, code: String },
    /// End an exchange: the key is erased here and the peer is told.
    Revoke { exchange: String },
    /// Rename an exchange.
    Label { exchange: String, label: String },
    /// Write an encrypted backup of the node identity and all exchanges.
    Export {
        #[arg(long)]
        out: PathBuf,
    },
    /// Restore exchanges (and the node identity) from a backup.
    Restore { file: PathBuf },
}

#[derive(Debug, Subcommand)]
pub enum NetShardCommand {
    /// Split the master key among peers: one sealed shard each, any THRESHOLD restore it.
    Entrust {
        /// Exchanges or peers (at least two different nodes).
        #[arg(long = "to", value_name = "EXCHANGE", required = true)]
        to: Vec<String>,
        /// Shards needed to restore (default: a majority, at least 2).
        #[arg(long, default_value_t = 0)]
        threshold: usize,
        #[command(flatten)]
        master: MasterInput,
    },
    /// Shards we entrusted, and shards we hold for others.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Gather shards back from peers and rebuild the master key.
    Restore {
        /// Use this exchange backup (when the database cannot be opened).
        #[arg(long)]
        backup: Option<PathBuf>,
        /// Only this shard set.
        #[arg(long)]
        set: Option<String>,
        /// Only these exchanges or peers.
        #[arg(long = "from", value_name = "EXCHANGE")]
        from: Vec<String>,
        /// Set a new Muninn password with the restored key (default with --backup).
        #[arg(long)]
        recover: bool,
        /// Print the 24 recovery words.
        #[arg(long)]
        print_words: bool,
    },
    /// Stop holding a shard for someone.
    Drop { id: String },
}

pub const CODE_NOTES: &str = "\
Every language becomes a native plugin (a Rust cdylib built with cargo, cached by content hash
under MUNINN_HOME/code); a prebuilt plugin (--lang plugin, the source is its path) loads as is.
Python is translated to Rust in process (no Python needed). Without a Rust toolchain only
prebuilt plugins run; `mnnn code check` says what is missing.

A code space runs as native code with your rights. Before its first run, and after every change,
review it and approve it (`approve`, or `run -y`). Imported code spaces need approving again.

Input: --model MODEL [--where \"FIELD OP VALUE\"]… [--matching TEXT] [--group F] [--sort F|-F]
[--limit N]; plugins can load, create and update any model (bars too), and create models.

Examples:
  mnnn code create fast --lang math --model bars --where \"symbol = BTC/USDT\" --group symbol --file fast.mth
  mnnn code run fast -y
  mnnn code create spread --lang python --source $'def spread(high: float, low: float) -> float:\\n    return high - low'
  mnnn rule create on-import --on import --then \"run-code fast\"";

/// A code space language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CodeLang {
    Math,
    Polars,
    Rust,
    Python,
    /// A prebuilt plugin (the source is the path of the library).
    Plugin,
}

impl CodeLang {
    pub fn lang(self) -> crate::store::Lang {
        use crate::store::Lang;
        match self {
            CodeLang::Math => Lang::Math,
            CodeLang::Polars => Lang::Polars,
            CodeLang::Rust => Lang::Rust,
            CodeLang::Python => Lang::Python,
            CodeLang::Plugin => Lang::Plugin,
        }
    }
}

/// Where a code space's source comes from (none: a starting template, or the editor with --editor).
#[derive(Debug, Clone, Default, Args)]
pub struct CodeSourceArgs {
    /// Read the source from a file (`-` reads standard input).
    #[arg(long, short = 'f', conflicts_with_all = ["source", "editor"])]
    pub file: Option<PathBuf>,
    /// The source itself.
    #[arg(long, conflicts_with = "editor")]
    pub source: Option<String>,
    /// Write the source in $VISUAL or $EDITOR.
    #[arg(long)]
    pub editor: bool,
}

/// The records a code space gets as input.
#[derive(Debug, Clone, Default, Args)]
pub struct CodeInputArgs {
    /// Model to read (`bars` or a designed model).
    #[arg(long)]
    pub model: Option<String>,
    /// Condition FIELD OP VALUE (repeatable; `;` separates several).
    #[arg(long = "where")]
    pub filter: Vec<String>,
    /// Records whose text fields contain this.
    #[arg(long)]
    pub matching: Option<String>,
    /// Group by these fields (math computes per group; repeatable or comma-separated).
    #[arg(long)]
    pub group: Vec<String>,
    /// Sort by these fields (`-F` descending; repeatable or comma-separated).
    #[arg(long, allow_hyphen_values = true)]
    pub sort: Vec<String>,
    /// At most this many records.
    #[arg(long)]
    pub limit: Option<usize>,
}

#[derive(Debug, Subcommand)]
pub enum CodeCommand {
    /// Create a code space (not approved to run yet).
    #[command(alias = "new")]
    Create {
        name: String,
        #[arg(long, value_enum)]
        lang: CodeLang,
        #[command(flatten)]
        source: CodeSourceArgs,
        #[command(flatten)]
        input: CodeInputArgs,
        #[arg(long, default_value = "")]
        description: String,
        /// Add it to a project.
        #[arg(long)]
        project: Option<String>,
    },
    /// Change a code space; a new source needs approving again.
    Edit {
        space: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long, value_enum)]
        lang: Option<CodeLang>,
        #[command(flatten)]
        source: CodeSourceArgs,
        /// Replace the input (with --model and friends).
        #[command(flatten)]
        input: CodeInputArgs,
        /// Take the input away (the plugin loads what it needs itself).
        #[arg(long, conflicts_with = "model")]
        no_input: bool,
    },
    /// List code spaces, with their scope (user, organization or global).
    #[command(alias = "ls")]
    List {
        /// Only this scope: user, org (any organization), org:NAME, or global.
        #[arg(long)]
        scope: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Show a code space: its source, input, approval and last run.
    Show {
        space: String,
        #[arg(long)]
        json: bool,
        /// Print only the source.
        #[arg(long, conflicts_with = "json")]
        source: bool,
    },
    /// Remove a code space (asks first).
    #[command(alias = "rm")]
    Remove { space: String },
    /// Approve a code space's current source to run.
    Approve { space: String },
    /// Compile a code space (cached: nothing happens when it is built already).
    Build { space: String },
    /// Run a code space: build if needed, then run it and show its result.
    Run {
        space: String,
        /// Print the run (status, view, logs) as JSON.
        #[arg(long)]
        json: bool,
        /// Table rows to print (0: all).
        #[arg(long, default_value_t = 30)]
        rows: usize,
    },
    /// Check a code space (or, without one, the toolchains) without running anything.
    Check { space: Option<String> },
    /// Write the plugin SDK (the `muninn-plugin` crate) to build prebuilt plugins with.
    Sdk {
        /// Directory to write it to.
        #[arg(long, default_value = "muninn-plugin")]
        out: PathBuf,
    },
    /// The built-in global executables shipped with Muninn (served by every hosted org).
    #[command(subcommand)]
    Builtin(CodeBuiltinCommand),
}

#[derive(Debug, Subcommand)]
pub enum CodeBuiltinCommand {
    /// The built-ins and their versions.
    List {
        #[arg(long)]
        json: bool,
    },
    /// A built-in's contract.
    Show { name: String },
    /// Override a built-in for one of your orgs: an editable code space of the same name,
    /// served in its place (asks first; it then runs as ordinary code, needing code.run).
    Override { name: String, org: String },
}

pub const MODEL_NOTES: &str = "\
Fields are NAME:TYPE with constraints: text, int, real, bool, timestamp, json, or ref:MODEL
(a record of another model). A trailing ! means required. Constraints: required, unique,
index, default=V, choices=a|b, about=\"…\"; to take one away: optional, -unique, -index,
-default, -choices, -about.

Changes go to a draft; `plan` shows the migration (+ add, - drop, ~ change), `apply` makes
it the next version, `history` lists versions and `rollback` applies an old design as a new
version. Steps that lose values ask first (-y says yes). The built-in bars model is edited
the same way: its fields from imports can be renamed, retyped, changed or dropped (the bars
keep their imported values; rollback brings a dropped field back).

Examples:
  mnnn model create signals --fields \"symbol:text!; price:real!; side:text choices=buy|sell\"
  mnnn model edit signals --add \"note:text\" --change \"price:real default=0\" --apply
  mnnn model plan signals
  mnnn model rollback signals 1
  mnnn model record create signals symbol=BTC/USDT price=42000 side=buy
  mnnn model record list signals --where \"price > 40000\" --sort -price
  mnnn model record list signals --group side --agg avg:price
  mnnn model record update signals --where \"side = buy\" note=checked";

#[derive(Debug, Subcommand)]
pub enum ModelCommand {
    /// List models (the built-in bars model too).
    #[command(alias = "ls")]
    List {
        #[arg(long)]
        json: bool,
    },
    /// List the field types.
    Types,
    /// Create a model (applied as v1 unless --draft).
    #[command(alias = "new")]
    Create {
        name: String,
        /// Field specs, `;`-separated or repeated: NAME:TYPE [required] [unique] [default=V]…
        #[arg(long = "fields", short = 'f', value_name = "SPECS")]
        fields: Vec<String>,
        #[arg(long, default_value = "")]
        description: String,
        /// Keep it as a draft (no table yet; `mnnn model apply` makes it v1).
        #[arg(long)]
        draft: bool,
        /// Add it to a project.
        #[arg(long)]
        project: Option<String>,
    },
    /// Change a model's draft (renames, then changes, drops, adds and moves), its name or
    /// description. --apply applies the draft right away.
    Edit {
        model: String,
        /// Add a field: NAME:TYPE [constraints]. Repeatable.
        #[arg(long, value_name = "SPEC")]
        add: Vec<String>,
        /// Change a field: NAME[:TYPE] [constraints] (what is not given stays). Repeatable.
        #[arg(long, value_name = "SPEC")]
        change: Vec<String>,
        /// Drop a field. Repeatable.
        #[arg(long, value_name = "FIELD")]
        drop: Vec<String>,
        /// Rename a field, OLD:NEW (values are kept). Repeatable.
        #[arg(long, value_name = "OLD:NEW")]
        rename: Vec<String>,
        /// Move a field, FIELD:POSITION (1 is first). Repeatable.
        #[arg(long = "move", value_name = "FIELD:N")]
        moves: Vec<String>,
        /// Rename the model (right away; not versioned).
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        /// Apply the draft afterwards.
        #[arg(long)]
        apply: bool,
    },
    /// Show a model: fields, draft, records and rules.
    Show {
        model: String,
        #[arg(long)]
        json: bool,
    },
    /// Show what applying the draft would do.
    Plan {
        model: String,
        /// Print the draft as a model-edit then-action.
        #[arg(long)]
        then: bool,
        #[arg(long)]
        json: bool,
    },
    /// Apply the draft as the next version (a migration). -y allows losing values.
    Apply {
        model: String,
        /// Only show the plan.
        #[arg(long)]
        dry_run: bool,
    },
    /// Throw the draft away.
    Discard { model: String },
    /// List the applied versions.
    History {
        model: String,
        #[arg(long)]
        json: bool,
    },
    /// Apply the design of an earlier version as the next version.
    Rollback {
        model: String,
        /// The version to go back to (2 or v2).
        version: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// Remove a model and its records (rules living in it become standalone, off).
    #[command(alias = "rm")]
    Remove { model: String },
    /// Records of a model: create, update, list, show, delete.
    #[command(subcommand, alias = "rec", alias = "records")]
    Record(RecordCommand),
}

#[derive(Debug, Subcommand)]
pub enum RecordCommand {
    /// Add a record: FIELD=VALUE… (defaults fill the rest).
    #[command(alias = "add", alias = "new")]
    Create {
        model: String,
        #[arg(value_name = "FIELD=VALUE")]
        values: Vec<String>,
        #[arg(long)]
        json: bool,
    },
    /// Change one record (--id) or every record matching --where: FIELD=VALUE… (empty
    /// value clears a field).
    Update {
        model: String,
        #[arg(long, required_unless_present = "conditions")]
        id: Option<String>,
        /// FIELD OP VALUE (repeatable, all must hold).
        #[arg(long = "where", value_name = "COND", conflicts_with = "id")]
        conditions: Vec<String>,
        #[arg(value_name = "FIELD=VALUE", required = true)]
        values: Vec<String>,
        /// Only count what would change.
        #[arg(long)]
        dry_run: bool,
    },
    /// List records: filter, find text, sort, group and page.
    #[command(alias = "ls")]
    List {
        model: String,
        /// FIELD OP VALUE (repeatable; ops = != ~ !~ > >= < <=, ? present, !? missing).
        #[arg(long = "where", value_name = "COND")]
        conditions: Vec<String>,
        /// Text found in any field.
        #[arg(long)]
        text: Option<String>,
        /// FIELD, -FIELD or FIELD:desc (comma-separated or repeated).
        #[arg(long, allow_hyphen_values = true)]
        sort: Vec<String>,
        /// Group by these fields (comma-separated or repeated).
        #[arg(long)]
        group: Vec<String>,
        /// With --group: sum:F, avg:F, min:F or max:F (repeatable).
        #[arg(long)]
        agg: Vec<String>,
        /// At most this many (bars: 50 unless given; 0 = all).
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long)]
        json: bool,
    },
    /// Show one record (id or id prefix).
    Show {
        model: String,
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Delete records by id, or every record matching --where (asks first).
    #[command(alias = "rm")]
    Delete {
        model: String,
        ids: Vec<String>,
        #[arg(long = "where", value_name = "COND")]
        conditions: Vec<String>,
        #[arg(long)]
        dry_run: bool,
    },
}

pub const ORG_NOTES: &str = "\
Orgs you create here are run by this node, and you are their owner. Orgs on other nodes
are joined by applying (an owner or admin approves) or by answering an invitation; both
are key exchanges scoped to the org. Commands that take ORG act on an org of yours, or
on a joined org through its node when you have the permission there.

Examples:
  mnnn org create \"Comet Lab\" --public
  mnnn org member invite comet-lab --peer ben --role user
  mnnn org apply /ip4/10.0.0.2/tcp/4741/p2p/12D3Koo… comet-lab --note \"hi\"
  mnnn org member approve comet-lab cleo --role viewer
  mnnn org resource add comet-lab journal \"field notes\"
  mnnn org publish comet-lab \"field notes\" --to public
  mnnn org edit comet-lab --hosted on && mnnn org host start
  mnnn org cert acme --domain lab.example.org --email me@example.org

See `mnnn help organizations` and `mnnn help org-hosting`.";

/// Organization commands.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Subcommand)]
pub enum OrgCommand {
    /// Create an organization (private, members only, until you say otherwise).
    Create {
        name: String,
        /// Its name in web addresses (default: from the name).
        #[arg(long)]
        slug: Option<String>,
        #[arg(long, default_value = "")]
        description: String,
        /// Listed and shown to anyone.
        #[arg(long)]
        public: bool,
        /// Anyone may act with the apply role, without joining.
        #[arg(long)]
        open: bool,
    },
    /// Change an organization.
    Edit {
        org: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        slug: Option<String>,
        #[arg(long)]
        description: Option<String>,
        /// public or private.
        #[arg(long)]
        visibility: Option<String>,
        /// members or open.
        #[arg(long)]
        access: Option<String>,
        /// Serve its site with `mnnn org host start`.
        #[arg(long, value_name = "on|off")]
        hosted: Option<OnOff>,
        /// Members may sign web requests with their exchange key.
        #[arg(long, value_name = "on|off")]
        member_auth: Option<OnOff>,
        /// A private org's site shows guests a landing, sign-in and join page.
        #[arg(long, value_name = "on|off")]
        gated: Option<OnOff>,
        /// The site template (a template name, or `default`).
        #[arg(long)]
        template: Option<String>,
    },
    /// Delete an organization of yours (its resources stay here as they are).
    Remove { org: String },
    /// Your organizations and the ones you joined.
    List {
        #[arg(long)]
        json: bool,
    },
    /// One organization: settings, roles, members, resources, projects, endpoints.
    Show {
        org: String,
        #[arg(long)]
        json: bool,
    },
    /// Roles and their permissions.
    #[command(subcommand)]
    Role(OrgRoleCommand),
    /// Members: applications, invitations, roles and kicks.
    #[command(subcommand)]
    Member(OrgMemberCommand),
    /// What an org holds (given to it, or added here).
    #[command(subcommand)]
    Resource(OrgResourceCommand),
    /// Put a resource on the org's site.
    Publish {
        org: String,
        resource: String,
        /// public or members.
        #[arg(long, default_value = "public")]
        to: String,
    },
    /// Take a resource off the org's site.
    Unpublish { org: String, resource: String },
    /// Projects of the org and their members.
    #[command(subcommand)]
    Project(OrgProjectCommand),
    /// Rules, code spaces and models that answer requests.
    #[command(subcommand)]
    Endpoint(OrgEndpointCommand),
    /// The org's code spaces this node runs (each node chooses its own).
    #[command(subcommand)]
    Code(OrgCodeCommand),
    /// Call an endpoint (of your org, or of a joined org through its node).
    Call {
        org: String,
        endpoint: String,
        /// JSON input (`-` reads stdin), e.g. '{\"op\":\"query\"}'.
        #[arg(long, default_value = "{}")]
        input: String,
    },
    /// Site templates and their assets.
    #[command(subcommand)]
    Template(OrgTemplateCommand),
    /// The organizations a node lists (public ones), or one by slug or id.
    Browse {
        /// The node (`…/p2p/ID`, a peer id or a known peer).
        target: String,
        org: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Apply to an organization on another node.
    Apply {
        target: String,
        org: String,
        /// A word for whoever approves.
        #[arg(long, default_value = "")]
        note: String,
    },
    /// Accept an invitation: the invite link, or the org name of one sent to this node (`-` reads stdin).
    Join { token: String },
    /// Check a joined org: an answer to your application, your role and permissions.
    Sync { org: String },
    /// Leave a joined org.
    Leave { org: String },
    /// Fetch a resource of a joined org.
    Fetch {
        org: String,
        resource: String,
        #[arg(long)]
        format: Option<String>,
        /// Write it here (default: stdout).
        #[arg(long, short)]
        out: Option<std::path::PathBuf>,
    },
    /// Give one of your things to a joined org (a copy, owned by the org).
    Give {
        org: String,
        /// model, search, journal, project, report or code.
        kind: String,
        name: String,
    },
    /// Copy a joined org's public site here to serve it (needs the host permission).
    Mirror { org: String },
    /// Copy a joined org's code spaces here to run them (needs host and code.view); changed
    /// ones ask for approval again.
    SyncCode {
        org: String,
        #[arg(long)]
        json: bool,
    },
    /// The `Authorization` header for a signed web request to a joined org's site.
    Sign {
        org: String,
        /// GET or POST.
        method: String,
        /// The path, e.g. /o/lab/api/artifacts.
        path: String,
        /// The request body (a file, `-` for stdin).
        #[arg(long)]
        body: Option<String>,
    },
    /// A one-time browser sign-in link to a joined org's site (valid five minutes, once).
    SignIn {
        org: String,
        /// The site's address, e.g. https://lab.example.org:4743.
        #[arg(long)]
        site: String,
    },
    /// Serve hosted orgs over HTTPS.
    #[command(subcommand)]
    Host(OrgHostCommand),
    /// The site's TLS certificate (self-signed, or ACME / Let's Encrypt).
    #[command(subcommand)]
    Cert(OrgCertCommand),
}

#[derive(Debug, Subcommand)]
pub enum OrgRoleCommand {
    List {
        org: String,
        #[arg(long)]
        json: bool,
    },
    /// The permissions roles can have.
    Perms,
    Create {
        org: String,
        name: String,
        /// Higher ranks manage lower ones (owner 100, admin 80, user 50, viewer 10).
        #[arg(long, default_value_t = 30)]
        rank: i64,
        /// A permission (repeatable; `*` for all).
        #[arg(long = "perm", value_name = "PERM")]
        perms: Vec<String>,
    },
    Edit {
        org: String,
        role: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        rank: Option<i64>,
        /// Replace the permissions (repeatable).
        #[arg(long = "perm", value_name = "PERM")]
        perms: Vec<String>,
        /// Remove every permission.
        #[arg(long, conflicts_with = "perms")]
        no_perms: bool,
    },
    /// Remove a role (its members get the lowest remaining role).
    Remove { org: String, role: String },
}

#[derive(Debug, Subcommand)]
pub enum OrgMemberCommand {
    /// The org's members, their roles and when they joined.
    List {
        org: String,
        #[arg(long)]
        json: bool,
    },
    /// Let an applicant in.
    Approve {
        org: String,
        member: String,
        /// Default: the org's apply role.
        #[arg(long)]
        role: Option<String>,
    },
    /// Turn an applicant away.
    Reject { org: String, member: String },
    /// Invite a node: prints the token to give them (or sends it with --peer).
    Invite {
        org: String,
        #[arg(long)]
        peer: Option<String>,
        #[arg(long, default_value = "")]
        label: String,
        /// Default: the org's invite role.
        #[arg(long)]
        role: Option<String>,
    },
    /// Remove a member; what they gave stays with the org.
    Kick {
        org: String,
        member: String,
        /// keep, archive or publish what they gave.
        #[arg(long, default_value = "keep")]
        resources: String,
    },
    /// Change a member's role.
    Role { org: String, member: String, role: String },
    /// The roles new members get.
    Defaults {
        org: String,
        /// For approved applicants (and guests of open orgs).
        #[arg(long)]
        apply: Option<String>,
        #[arg(long)]
        invite: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum OrgResourceCommand {
    /// What the org holds, and who gave it.
    List {
        org: String,
        #[arg(long)]
        json: bool,
    },
    /// Give the org one of your things here (the original stays as it is).
    Add {
        org: String,
        /// model, search, journal, project, report or code.
        kind: String,
        name: String,
    },
    /// Hide a resource from members (it is kept; restore brings it back).
    Archive { org: String, resource: String },
    /// Bring an archived resource back for members.
    Restore { org: String, resource: String },
    /// No longer the org's (the thing itself stays here).
    Release { org: String, resource: String },
}

#[derive(Debug, Subcommand)]
pub enum OrgProjectCommand {
    List {
        org: String,
    },
    Add {
        org: String,
        project: String,
    },
    Remove {
        org: String,
        project: String,
    },
    /// Add a member to an org project (their role there never exceeds their org role).
    Member {
        org: String,
        project: String,
        member: String,
        #[arg(long)]
        role: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum OrgEndpointCommand {
    /// The org's callable endpoints, their targets and permissions.
    List {
        org: String,
        #[arg(long)]
        json: bool,
    },
    /// Expose a model's records, a rule or a code space as a callable endpoint.
    Add {
        org: String,
        name: String,
        /// records, rule or code.
        #[arg(long)]
        kind: String,
        /// The model, rule or code space.
        #[arg(long)]
        target: String,
        /// The permission callers need (default: records.read for records, else call).
        #[arg(long)]
        perm: Option<String>,
        /// Callable without signing in (public or open orgs).
        #[arg(long)]
        anyone: bool,
    },
    /// Remove an endpoint (callers are then refused).
    Remove { org: String, name: String },
}

#[derive(Debug, Subcommand)]
pub enum OrgCodeCommand {
    /// The code spaces this node serves for the org.
    List {
        org: String,
        #[arg(long)]
        json: bool,
    },
    /// Serve a code space for the org on this node.
    Serve { org: String, space: String },
    /// Stop serving it here (its endpoints then refuse calls on this node).
    Stop { org: String, space: String },
}

#[derive(Debug, Subcommand)]
pub enum OrgTemplateCommand {
    List {
        org: String,
    },
    /// Save a template (placeholders: {{title}} {{org.name}} {{org.description}} {{content}} {{css}} {{assets}} {{base}}).
    Save {
        org: String,
        name: String,
        #[arg(long)]
        html: std::path::PathBuf,
        #[arg(long)]
        css: Option<std::path::PathBuf>,
        /// Use it for the org's site.
        #[arg(long)]
        r#use: bool,
    },
    /// Add an asset (image, font, script, style) to a template.
    Asset {
        org: String,
        template: String,
        file: std::path::PathBuf,
        /// Its name under /assets/ (default: the file name).
        #[arg(long)]
        name: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum OrgHostCommand {
    /// Serve in the foreground until Ctrl-C (or --for SECS, or `mnnn org host stop`).
    Start {
        /// Listen address (default from settings: 127.0.0.1).
        #[arg(long)]
        bind: Option<String>,
        /// HTTPS port (default from settings: 8443; 0 picks one).
        #[arg(long)]
        port: Option<u16>,
        #[arg(long = "for", value_name = "SECS")]
        for_secs: Option<u64>,
    },
    Stop,
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Serve the site whenever the node starts (`on`), or only when asked (`off`, the default).
    Autostart {
        state: OnOff,
    },
}

#[derive(Debug, Subcommand)]
pub enum OrgCertCommand {
    /// The certificate in use (or that would be).
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Set up ACME (Let's Encrypt), or turn it off with --off.
    Acme {
        /// The public name the certificate is for (it must point at this machine).
        #[arg(long)]
        domain: Option<String>,
        /// Contact address given to the CA.
        #[arg(long)]
        email: Option<String>,
        /// letsencrypt, letsencrypt-staging or a directory URL.
        #[arg(long)]
        directory: Option<String>,
        /// Certificate profile (shortlived; an empty value for the CA's default).
        #[arg(long)]
        profile: Option<String>,
        /// Port the HTTP-01 challenge is answered on.
        #[arg(long)]
        http_port: Option<u16>,
        /// Turn ACME off (back to self-signed certificates).
        #[arg(long)]
        off: bool,
    },
    /// Get an ACME certificate now (needs the challenge port reachable as port 80).
    Renew,
}

pub const REVIEW_NOTES: &str = "\
A review request is about one thing of yours: an entry, a journal (and its entries), a
project, a live search or a model record (MODEL/RECORD). Each link it makes is a key
exchange scoped to that review, good for one reviewer for a week. Reviewers reach the
review through your node while `mnnn net start` runs. Commands that take REVIEW act on a
review of yours, or on one you joined (through its author's node).

Examples:
  mnnn review request journal \"field notes\" --note \"before Friday\" --allow comment,suggest
  mnnn review invite \"field notes\" --label ben
  mnnn review join mnnn-x1-…
  mnnn review suggest \"field notes\" --entry 3f2a --at \"the cat\" --to \"the comet\"
  mnnn review vote \"field notes\" approve
  mnnn review accept \"field notes\" --all && mnnn review merge \"field notes\"
  mnnn review revisions journal \"field notes\"

See `mnnn help reviews`.";

/// Where a comment, suggestion or highlight points.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct ReviewAnchor {
    /// The field (body, title, name, description, query, a record field…).
    #[arg(long)]
    pub field: Option<String>,
    /// Anchor on this text of the field.
    #[arg(long)]
    pub at: Option<String>,
    /// Which occurrence of --at (1-based).
    #[arg(long, default_value_t = 1)]
    pub nth: usize,
    /// Journal reviews: the entry (an id or id prefix).
    #[arg(long)]
    pub entry: Option<String>,
}

/// Review commands.
#[derive(Debug, Subcommand)]
pub enum ReviewCommand {
    /// Ask for a review of an entry, journal, project, search or record (makes a link).
    Request {
        /// entry, journal, project, search or record.
        kind: String,
        /// Its name or id (records: MODEL/RECORD).
        target: String,
        #[arg(long, default_value = "")]
        title: String,
        /// A word for reviewers.
        #[arg(long, default_value = "")]
        note: String,
        /// What reviewers may do: view, comment, suggest, highlight, vote (default: all).
        #[arg(long, value_delimiter = ',')]
        allow: Vec<String>,
        /// Who the first link is for.
        #[arg(long, default_value = "")]
        label: String,
        /// Send the link to this peer (a known peer, peer id or address).
        #[arg(long)]
        peer: Option<String>,
        /// Make no link yet (`mnnn review invite` makes them).
        #[arg(long)]
        no_link: bool,
    },
    /// Make another link for a review (one per reviewer).
    Invite {
        review: String,
        #[arg(long, default_value = "")]
        label: String,
        #[arg(long)]
        peer: Option<String>,
    },
    /// Your reviews and the ones you joined.
    List {
        #[arg(long)]
        json: bool,
    },
    /// One review: what it is about, its reviewers, votes and thread.
    Show {
        review: String,
        #[arg(long)]
        json: bool,
    },
    /// What is under review (its fields; journals: its entries).
    View {
        review: String,
        #[arg(long)]
        json: bool,
    },
    /// Join a review: the review link, or the title of an invitation sent to this node (`-` reads stdin).
    Join {
        token: String,
        /// What to call it here.
        #[arg(long, default_value = "")]
        label: String,
    },
    /// Check a joined review: its title, what you may do, open or closed.
    Sync { review: String },
    /// Comment (on the whole thing, a field, or text in it with --at).
    Comment {
        review: String,
        text: String,
        #[command(flatten)]
        anchor: ReviewAnchor,
        /// Reply to a thread item.
        #[arg(long)]
        reply: Option<String>,
    },
    /// Suggest a change: replace the --at text, or the whole field, with --to.
    Suggest {
        review: String,
        /// The new text (or value).
        #[arg(long)]
        to: String,
        /// --to is JSON (a number, list…; for record fields).
        #[arg(long)]
        json: bool,
        /// Why.
        #[arg(long, default_value = "")]
        why: String,
        #[command(flatten)]
        anchor: ReviewAnchor,
    },
    /// Highlight text (--at) of a field.
    Highlight {
        review: String,
        #[command(flatten)]
        anchor: ReviewAnchor,
        /// A color or tag.
        #[arg(long)]
        color: Option<String>,
        #[arg(long, default_value = "")]
        note: String,
    },
    /// Vote on the review (approve, changes, reject) or on an item with --on (up, down).
    Vote {
        review: String,
        vote: String,
        #[arg(long)]
        on: Option<String>,
        #[arg(long, default_value = "")]
        note: String,
    },
    /// Take back one of your items.
    Withdraw { review: String, item: String },
    /// Accept suggestions (items, or every open one with --all).
    Accept {
        review: String,
        items: Vec<String>,
        #[arg(long)]
        all: bool,
    },
    /// Reject suggestions (items, or every open one with --all).
    Reject {
        review: String,
        items: Vec<String>,
        #[arg(long)]
        all: bool,
    },
    /// Merge the accepted suggestions into the thing (a new revision).
    Merge {
        review: String,
        /// Apply suggestions even where the field changed since.
        #[arg(long)]
        force: bool,
    },
    /// Close a review (reviewers' links and exchanges end).
    Close { review: String },
    /// Open a closed review again (reviewers need new links).
    Reopen { review: String },
    /// Take a reviewer off a review.
    Drop { review: String, reviewer: String },
    /// Leave a review you joined.
    Leave { review: String },
    /// Delete a review and its thread (asks first; revisions stay).
    Remove { review: String },
    /// The revisions of a thing (merged reviews).
    Revisions {
        kind: String,
        target: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Args)]
pub struct ShareArgs {
    /// What to share: model, search, journal, project, report or code.
    pub kind: String,
    /// The item (id or name).
    #[arg(required_unless_present = "targets")]
    pub item: Option<String>,
    /// Where: project, journal, code or net.
    #[arg(required_unless_present = "targets")]
    pub target: Option<String>,
    /// The project, journal or code space (id or name); for net, exchanges (none: all).
    pub to: Vec<String>,
    /// A note for a journal entry.
    #[arg(long, default_value = "")]
    pub note: String,
    /// List where this kind of item can be shared.
    #[arg(long)]
    pub targets: bool,
    #[arg(long)]
    pub json: bool,
}
