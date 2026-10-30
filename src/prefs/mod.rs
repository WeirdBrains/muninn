//! `preferences.toml`.
use crate::error::Result;
use crate::paths::{Paths, write_private};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    Dark,
    #[default]
    System,
}

impl std::fmt::Display for ThemeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
            ThemeMode::System => "system",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemePrefs {
    pub name: String,
    pub mode: ThemeMode,
}

impl Default for ThemePrefs {
    fn default() -> Self {
        Self { name: "tokyo-night".into(), mode: ThemeMode::System }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct IngestPrefs {
    /// Drop zero-volume bars instead of keeping them flagged.
    #[serde(default)]
    pub drop_zero_volume: bool,
}

/// TUI header size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum HeaderMode {
    /// Large when the terminal has at least 30 rows.
    #[default]
    Auto,
    Compact,
    Large,
}

impl HeaderMode {
    pub fn next(self) -> Self {
        match self {
            HeaderMode::Auto => HeaderMode::Compact,
            HeaderMode::Compact => HeaderMode::Large,
            HeaderMode::Large => HeaderMode::Auto,
        }
    }
}

/// TUI state remembered between sessions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TuiPrefs {
    #[serde(default)]
    pub header: HeaderMode,
    #[serde(default)]
    pub sidebar_collapsed: bool,
    /// The history bar on the left (View ▸ History, `Y`).
    #[serde(default)]
    pub history_open: bool,
    /// Bookmarked import folders (Places).
    #[serde(default)]
    pub import_bookmarks: Vec<String>,
    /// Recently imported folders (newest first).
    #[serde(default)]
    pub import_recent: Vec<String>,
    /// Sidebar sort and filters.
    #[serde(default)]
    pub sidebar: crate::tui::sidebar::SidebarPrefs,
    /// Current tiling layout (restored on start).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<crate::tui::layout::TileLayout>,
    /// Named layouts (View ▸ Layouts ▸ Save current…).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub layouts: std::collections::BTreeMap<String, crate::tui::layout::TileLayout>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Preferences {
    #[serde(default)]
    pub theme: ThemePrefs,
    #[serde(default)]
    pub ingest: IngestPrefs,
    #[serde(default)]
    pub tui: TuiPrefs,
    /// The network node (`mnnn net config`, Network ▸ Settings).
    #[serde(default)]
    pub net: NetPrefs,
    /// Organization web hosting (`mnnn org host`, `[hosting]`).
    #[serde(default)]
    pub hosting: HostingPrefs,
}

/// Organization web hosting. Nothing is served until `mnnn org host start`, and then only
/// on this machine (`127.0.0.1`) unless `bind` says otherwise.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HostingPrefs {
    /// Address to listen on (`0.0.0.0` or `::` to serve other machines).
    pub bind: String,
    /// HTTPS port (HTML, JSON and gRPC share it).
    pub port: u16,
    /// Extra names for the self-signed certificate (`localhost` and `bind` are always in it).
    pub names: Vec<String>,
    /// How long a self-signed certificate lasts, in days (a new one is made at each start).
    pub cert_days: u32,
    /// Let's Encrypt (or another ACME CA), off until enabled with a domain and an email.
    pub acme: AcmePrefs,
}

impl Default for HostingPrefs {
    fn default() -> Self {
        HostingPrefs { bind: "127.0.0.1".into(), port: 8443, names: vec![], cert_days: 7, acme: AcmePrefs::default() }
    }
}

/// ACME certificates (Let's Encrypt short-lived certificates by default).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcmePrefs {
    pub enabled: bool,
    /// The public name the certificate is for (it must point at this machine).
    pub domain: String,
    /// Contact address given to the CA.
    pub email: String,
    /// `letsencrypt`, `letsencrypt-staging` or a directory URL.
    pub directory: String,
    /// Certificate profile (`shortlived`: about six days; empty: the CA's default).
    pub profile: String,
    /// Port the HTTP-01 challenge is answered on (the CA always asks port 80; forward it here).
    pub http_port: u16,
}

impl Default for AcmePrefs {
    fn default() -> Self {
        AcmePrefs {
            enabled: false,
            domain: String::new(),
            email: String::new(),
            directory: "letsencrypt".into(),
            profile: "shortlived".into(),
            http_port: 80,
        }
    }
}

impl AcmePrefs {
    /// The ACME directory URL.
    pub fn directory_url(&self) -> String {
        match self.directory.as_str() {
            "" | "letsencrypt" => "https://acme-v02.api.letsencrypt.org/directory".into(),
            "letsencrypt-staging" => "https://acme-staging-v02.api.letsencrypt.org/directory".into(),
            u => u.to_string(),
        }
    }
}

/// Network node settings. Nothing starts or is shared until you say so.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NetPrefs {
    /// How this node introduces itself to peers (empty: the author's name, else "muninn").
    pub label: String,
    /// TCP and QUIC port (0: let the system pick).
    pub port: u16,
    /// Listen addresses replacing the default (all interfaces on `port`).
    pub listen: Vec<String>,
    /// Find peers on the local network.
    pub mdns: bool,
    /// Relay traffic for other nodes and announce it.
    pub relay_server: bool,
    /// Relays to reserve a slot on (`…/p2p/ID`).
    pub relays: Vec<String>,
    /// Nodes to join the wide network through (`…/p2p/ID`).
    pub bootstrap: Vec<String>,
    /// Start the node when the TUI opens.
    pub autostart: bool,
}

impl Default for NetPrefs {
    fn default() -> Self {
        NetPrefs {
            label: String::new(),
            port: crate::net::node::DEFAULT_PORT,
            listen: vec![],
            mdns: true,
            relay_server: false,
            relays: vec![],
            bootstrap: vec![],
            autostart: false,
        }
    }
}

impl NetPrefs {
    /// The node configuration these settings describe.
    pub fn node_config(&self, label: &str) -> Result<crate::net::NodeConfig> {
        let listen = if self.listen.is_empty() {
            crate::net::NodeConfig::default_listen(self.port)
        } else {
            self.listen.iter().map(|a| a.parse().map_err(|e| crate::error::Error::invalid("listen", format!("{a}: {e}")))).collect::<Result<_>>()?
        };
        let addrs = |v: &[String], what: &'static str| -> Result<Vec<libp2p::Multiaddr>> {
            v.iter().map(|a| a.parse().map_err(|e| crate::error::Error::invalid(what, format!("{a}: {e}")))).collect()
        };
        Ok(crate::net::NodeConfig {
            listen,
            mdns: self.mdns,
            relay_server: self.relay_server,
            bootstrap: addrs(&self.bootstrap, "bootstrap")?,
            relays: addrs(&self.relays, "relay")?,
            memory: false,
            label: label.to_string(),
        })
    }
}

impl Preferences {
    /// Load preferences, creating the file with defaults when missing.
    pub fn load_or_create(paths: &Paths) -> Result<Self> {
        let p = paths.preferences();
        if p.exists() {
            let s = std::fs::read_to_string(&p)?;
            Ok(toml::from_str(&s)?)
        } else {
            let prefs = Preferences::default();
            prefs.save(paths)?;
            Ok(prefs)
        }
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        let s = toml::to_string_pretty(self)?;
        write_private(&paths.preferences(), s.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_tokyo_night_system() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::resolve(Some(tmp.path().to_path_buf()), None).unwrap();
        paths.ensure().unwrap();
        let p = Preferences::load_or_create(&paths).unwrap();
        assert_eq!(p.theme.name, "tokyo-night");
        assert_eq!(p.theme.mode, ThemeMode::System);
        let text = std::fs::read_to_string(paths.preferences()).unwrap();
        assert!(text.contains("name = \"tokyo-night\""));
        assert!(text.contains("mode = \"system\""));
        // the network is off and shares nothing until asked
        assert!(!p.net.autostart);
        assert!(p.net.mdns && !p.net.relay_server);
        assert_eq!(p.net.port, 4741);
        // older files without [net] still load
        let old: Preferences = toml::from_str("[theme]\nname = \"x\"\nmode = \"dark\"\n").unwrap();
        assert_eq!(old.net, NetPrefs::default());
    }
}
