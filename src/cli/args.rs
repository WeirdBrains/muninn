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
    /// Launch the terminal UI.
    Tui,
    /// Show a help topic (the same pages as the TUI help and docs/).
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
    /// Source name (created if unknown).
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
}

#[derive(Debug, Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum IngestCommand {
    /// Ingest CSV file(s).
    Csv(CsvIngestArgs),
    /// Ingest JSON / NDJSON file(s).
    Json(JsonIngestArgs),
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
