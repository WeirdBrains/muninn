//! `mnnn net`: the network. Thin over [`crate::net`].
use super::{Ctx, NetCommand, NetExchangeCommand, NetRelayCommand, NetShardCommand};
use crate::crypto::{self, Key32, keyring};
use crate::ingest::time::fmt_ts;
use crate::net::{self, Node, auto, client, identity, local};
use crate::org::host;
use crate::prefs::Preferences;
use crate::store::{ExchangeStatus, NewAction, SearchSpec, ShareKind, Store};
use anyhow::{Context, anyhow};
use libp2p::identity::Keypair;
use secrecy::SecretString;
use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const ORIGIN: &str = "cli";

/// What most net commands need: the store, our identity and (made on first use) a node.
pub(super) struct Net<'a> {
    ctx: &'a Ctx,
    dek: Key32,
    pub(super) store: Store,
    pub(super) prefs: Preferences,
    pub(super) kp: Keypair,
    pub(super) label: String,
    pub(super) node: Option<Node>,
}

impl<'a> Net<'a> {
    pub(super) fn open(ctx: &'a Ctx) -> anyhow::Result<Self> {
        let dek = ctx.unlock()?;
        let store = Store::open(&ctx.paths.db, Some(&dek.0))?;
        store.set_rule_echo(!ctx.global.quiet);
        let prefs = ctx.prefs()?;
        let kp = identity::load_or_create(&store)?;
        let label = local::my_label(&store, &prefs.net);
        Ok(Net { ctx, dek, store, prefs, kp, label, node: None })
    }

    pub(super) fn second_store(&self) -> anyhow::Result<Store> {
        Ok(Store::open(&self.ctx.paths.db, Some(&self.dek.0))?)
    }

    pub(super) fn relays(&self) -> Vec<String> {
        self.prefs.net.relays.clone()
    }

    /// A short-lived node for this command.
    pub(super) fn node(&mut self) -> anyhow::Result<&Node> {
        if self.node.is_none() {
            let svc = self.second_store()?;
            self.node = Some(local::ephemeral(&self.kp, svc, &self.label, &self.prefs.net)?);
        }
        Ok(self.node.as_ref().expect("node"))
    }

    pub(super) fn addrs(&self) -> Vec<String> {
        local::offer_addrs(&self.ctx.paths.home, &self.store, None)
    }
}

pub(super) fn kind(k: &str) -> anyhow::Result<ShareKind> {
    ShareKind::parse(k).ok_or_else(|| anyhow!("`{k}`: share a model, search, journal, project, report or code"))
}

pub(super) fn read_token(input: &str) -> anyhow::Result<String> {
    if input == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        return Ok(s.trim().to_string());
    }
    Ok(input.to_string())
}

/// The password that seals dark shards: MUNINN_SHARD_PASSWORD, else the export password.
fn shard_password(ctx: &Ctx) -> anyhow::Result<SecretString> {
    if let Ok(p) = std::env::var("MUNINN_SHARD_PASSWORD") {
        return Ok(p.into());
    }
    ctx.export_password("Shard password")
}

fn print_status(s: &net::Status, label: &str, running: Option<u32>) {
    println!("node:       {label}");
    println!("peer id:    {}", s.peer_id);
    match running {
        Some(pid) => println!("running:    yes (pid {pid}, since {})", fmt_ts(s.started_at)),
        None => println!("running:    no (start with `mnnn net start`)"),
    }
    if running.is_some() {
        println!("nat:        {}", s.nat);
        println!("mdns:       {}", if s.mdns { "on" } else { "off" });
        println!("relay:      {}", if s.relay_server { "serving" } else { "not serving" });
        for a in local::reachable(s) {
            println!("address:    {a}");
        }
        for r in &s.reservations {
            println!("reserved:   {r}");
        }
        println!("connected:  {}", s.connected.len());
    }
}

pub fn run(ctx: &Ctx, cmd: NetCommand) -> anyhow::Result<()> {
    match cmd {
        NetCommand::Start { port, listen, no_mdns, relay_server, for_secs } => start(ctx, port, listen, no_mdns, relay_server, for_secs),
        NetCommand::Stop => {
            let Some(f) = local::running(&ctx.paths.home) else {
                ctx.io.info("no node is running");
                return Ok(());
            };
            local::request_stop(&ctx.paths.home)?;
            let t = Instant::now();
            while local::running(&ctx.paths.home).is_some() && t.elapsed() < Duration::from_secs(15) {
                std::thread::sleep(Duration::from_millis(200));
            }
            if local::running(&ctx.paths.home).is_some() {
                return Err(anyhow!("the node (pid {}) did not stop; stop it with Ctrl-C", f.pid));
            }
            ctx.io.info(format!("node {} stopped", f.label));
            Ok(())
        }
        NetCommand::Status { json } => {
            let n = Net::open(ctx)?;
            let running = local::running(&ctx.paths.home);
            let status = running
                .as_ref()
                .map(|f| f.status.clone())
                .unwrap_or_else(|| net::Status { peer_id: n.kp.public().to_peer_id().to_string(), ..Default::default() });
            let xs = n.store.exchanges()?;
            let active = xs.iter().filter(|x| x.status.active()).count();
            let incoming = xs.iter().filter(|x| x.status == ExchangeStatus::Incoming).count();
            let shares = n.store.shares()?.len();
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "label": n.label,
                        "running": running.as_ref().map(|f| f.pid),
                        "status": status,
                        "fingerprint": identity::fingerprint(&n.kp.public()),
                        "exchanges": active,
                        "incoming": incoming,
                        "shared": shares,
                        "settings": n.prefs.net,
                    }))?
                );
            } else {
                print_status(&status, &n.label, running.as_ref().map(|f| f.pid));
                println!("identity:   {}", identity::fingerprint(&n.kp.public()));
                println!("exchanges:  {active} active{}", if incoming > 0 { format!(", {incoming} incoming offer(s)") } else { String::new() });
                println!("shared:     {shares} item(s)");
            }
            Ok(())
        }
        NetCommand::Peers { json } => {
            let n = Net::open(ctx)?;
            let ps = n.store.net_peers()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&ps)?);
            } else {
                for p in &ps {
                    let st = auto::node_link(&n.store, &p.peer_id)?.map(|x| auto::status_of(&x).as_str()).unwrap_or("-");
                    println!(
                        "{}\t{}\t{}\t{}\t{}\tseen {}",
                        p.short_id(),
                        if p.label.is_empty() { "-" } else { &p.label },
                        client::caps_text(&p.caps),
                        st,
                        p.source,
                        fmt_ts(p.last_seen)
                    );
                }
                if ps.is_empty() {
                    ctx.io.info("no peers yet: start the node (`mnnn net start`) or `mnnn net connect ADDR`");
                }
            }
            Ok(())
        }
        NetCommand::Connect { target, label, hello } => {
            let mut n = Net::open(ctx)?;
            let relays = n.relays();
            n.node()?;
            if hello {
                let p = client::hello(&n.store, n.node.as_ref().expect("node"), &target, label.as_deref(), &relays)?;
                ctx.io.info(format!("{} ({}) says hello: {}", p.label, p.short_id(), p.agent));
                return Ok(());
            }
            let addrs = n.addrs();
            if net::exchange::is_token(&net::exchange::unlink(&target)) {
                let l = auto::use_link(&n.store, n.node.as_ref(), &n.kp, &target, label.as_deref().unwrap_or(""), &n.label, addrs, ORIGIN)?;
                print_link(ctx, &l);
                return Ok(());
            }
            ctx.io.note(format!("connecting to {target}…"));
            let l = auto::connect(&n.store, n.node.as_ref().expect("node"), &n.kp, &target, label.as_deref(), &n.label, addrs, &relays, ORIGIN)?;
            print_link(ctx, &l);
            if l.status == auto::LinkStatus::Failed {
                return Err(anyhow!("{}", l.detail));
            }
            Ok(())
        }
        NetCommand::Invite { label, json } => {
            let n = Net::open(ctx)?;
            let relays = n.relays();
            let addrs = n.addrs();
            let c = client::create(
                &n.store,
                None,
                &n.kp,
                label.as_deref().unwrap_or(""),
                &n.label,
                addrs,
                None,
                client::DEFAULT_TTL_MS,
                &relays,
                ORIGIN,
            )?;
            let link = net::exchange::link(&c.token);
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({ "link": link, "id": c.exchange.id, "expires_at": c.exchange.expires_at }))?
                );
            } else {
                println!("{link}");
                ctx.io.note("send it to them; when they use it the nodes connect (it holds no secret)");
            }
            Ok(())
        }
        NetCommand::Requests { json } => {
            let n = Net::open(ctx)?;
            let reqs = auto::requests(&n.store)?;
            if json {
                let v: Vec<_> = reqs
                    .iter()
                    .map(|x| serde_json::json!({ "id": x.id, "from": x.title(), "peer": x.peer_id, "for": auto::purpose(x), "at": x.created_at }))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&v)?);
                return Ok(());
            }
            for x in &reqs {
                println!("{}\t{}\t{}\t{}", &x.id[..8.min(x.id.len())], x.title(), auto::purpose(x), fmt_ts(x.created_at));
            }
            if reqs.is_empty() {
                ctx.io.info("no requests waiting");
            }
            Ok(())
        }
        NetCommand::Accept { request } => {
            let mut n = Net::open(ctx)?;
            n.node()?;
            let addrs = n.addrs();
            let l = if net::exchange::is_token(&net::exchange::unlink(&request)) {
                auto::use_link(&n.store, n.node.as_ref(), &n.kp, &request, "", &n.label, addrs, ORIGIN)?
            } else {
                auto::accept(&n.store, n.node.as_ref(), &n.kp, &request, &n.label, addrs, ORIGIN)?
            };
            print_link(ctx, &l);
            Ok(())
        }
        NetCommand::Decline { request } => {
            let n = Net::open(ctx)?;
            let x = auto::decline(&n.store, &request, ORIGIN)?;
            ctx.io.info(format!("declined {} ({})", x.title(), auto::purpose(&x)));
            Ok(())
        }
        NetCommand::Disconnect { peer } => {
            let mut n = Net::open(ctx)?;
            let relays = n.relays();
            let key = match n.store.find_net_peer(&peer).ok().and_then(|p| auto::node_link(&n.store, &p.peer_id).ok().flatten()) {
                Some(x) => x.id,
                None => peer.clone(),
            };
            n.node()?;
            let (x, told) = client::revoke(&n.store, n.node.as_ref(), &key, &relays, ORIGIN)?;
            ctx.io.info(format!("disconnected from {}", x.title()));
            if let Some(Err(e)) = told {
                ctx.io.warn(format!("{} was not reachable ({}); it will find out when it next asks", x.title(), auto::plain(&e)));
            }
            Ok(())
        }
        NetCommand::Forget { peer } => {
            let n = Net::open(ctx)?;
            let p = n.store.find_net_peer(&peer)?;
            n.store.remove_net_peer(&p.peer_id)?;
            ctx.io.info(format!("forgot {}", if p.label.is_empty() { p.short_id().to_string() } else { p.label }));
            Ok(())
        }
        NetCommand::Relay(c) => relay(ctx, c),
        NetCommand::Config { label, port, listen, mdns, autostart, accept, bootstrap, clear_bootstrap, json } => {
            let mut prefs = ctx.prefs()?;
            let before = prefs.net.clone();
            let n = &mut prefs.net;
            if let Some(l) = label {
                n.label = l.trim().to_string();
            }
            if let Some(p) = port {
                n.port = p;
            }
            if !listen.is_empty() {
                n.listen = if listen.iter().any(|l| l == "default") { vec![] } else { listen };
            }
            if let Some(m) = mdns {
                n.mdns = m.on();
            }
            if let Some(a) = autostart {
                n.autostart = a.on();
            }
            if let Some(a) = accept {
                let a = a.trim().to_ascii_lowercase();
                if a != "known" && a != "anyone" {
                    return Err(anyhow!("--accept: `known` or `anyone`"));
                }
                n.accept = a;
            }
            if clear_bootstrap {
                n.bootstrap.clear();
            }
            for b in bootstrap {
                if !n.bootstrap.contains(&b) {
                    n.bootstrap.push(b);
                }
            }
            n.node_config("check")?;
            if prefs.net != before {
                prefs.save(&ctx.paths)?;
                if let Some(s) = ctx.store_if_unlocked() {
                    s.note_action(&NewAction::new("net.config", ORIGIN, "network settings changed").params(serde_json::to_value(&prefs.net)?));
                }
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&prefs.net)?);
            } else {
                let n = &prefs.net;
                println!("label:        {}", if n.label.is_empty() { "(author name)" } else { &n.label });
                println!("port:         {}", n.port);
                println!("listen:       {}", if n.listen.is_empty() { "all interfaces".to_string() } else { n.listen.join(" ") });
                println!("mdns:         {}", if n.mdns { "on" } else { "off" });
                println!("relay server: {}", if n.relay_server { "on" } else { "off" });
                println!("relays:       {}", if n.relays.is_empty() { "-".to_string() } else { n.relays.join(" ") });
                println!("bootstrap:    {}", if n.bootstrap.is_empty() { "-".to_string() } else { n.bootstrap.join(" ") });
                println!("autostart:    {}", if n.autostart { "on (TUI)" } else { "off" });
                println!("accept:       {}", if n.accept_anyone() { "anyone" } else { "known nodes (others ask first)" });
            }
            Ok(())
        }
        NetCommand::Exchange(c) => exchange(ctx, c),
        NetCommand::Share { kind: k, item, with } => {
            let mut n = Net::open(ctx)?;
            // not connected yet: connect first (it may wait for the other side)
            for w in &with {
                if auto::link_for(&n.store, w).is_err() {
                    let relays = n.relays();
                    let addrs = n.addrs();
                    n.node()?;
                    ctx.io.note(format!("connecting to {w}…"));
                    let l = auto::connect(&n.store, n.node.as_ref().expect("node"), &n.kp, w, None, &n.label, addrs, &relays, ORIGIN)?;
                    print_link(ctx, &l);
                    if l.status == auto::LinkStatus::Failed {
                        return Err(anyhow!("{}", l.detail));
                    }
                }
            }
            let made = client::share(&n.store, kind(&k)?, &item, &with, ORIGIN)?;
            let fresh = made.iter().filter(|(_, new)| *new).count();
            let s = &made[0].0;
            ctx.io.info(format!(
                "sharing {} {} with {}{}",
                s.kind.label(),
                s.name,
                if with.is_empty() { "every key exchange".to_string() } else { with.join(", ") },
                if fresh == 0 { " (already)" } else { "" }
            ));
            Ok(())
        }
        NetCommand::Unshare { kind: k, item, with } => {
            let n = Net::open(ctx)?;
            let gone = client::unshare(&n.store, kind(&k)?, &item, &with, ORIGIN)?;
            ctx.io.info(format!("stopped sharing {} {}", gone[0].kind.label(), gone[0].name));
            Ok(())
        }
        NetCommand::Shared { json } => {
            let n = Net::open(ctx)?;
            let all = n.store.shares()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&all)?);
            } else {
                for s in &all {
                    let with = match &s.exchange_id {
                        None => "everyone with an exchange".to_string(),
                        Some(x) => n.store.exchange(x)?.map(|x| x.title()).unwrap_or_else(|| x.clone()),
                    };
                    println!("{}\t{}\t{}\t{with}", s.short_id(), s.kind.label(), s.name);
                }
                if all.is_empty() {
                    ctx.io.info("nothing is shared; `mnnn net share KIND NAME` shares an item");
                }
            }
            Ok(())
        }
        NetCommand::List { exchange, json } => {
            let mut n = Net::open(ctx)?;
            let relays = n.relays();
            let xs = match exchange {
                Some(k) => vec![client::exchange_for(&n.store, &k)?],
                None => n.store.exchanges()?.into_iter().filter(|x| x.status.active()).collect(),
            };
            n.node()?;
            let node = n.node.as_ref().expect("node");
            let mut out = vec![];
            for x in &xs {
                match client::remote_list(&n.store, node, x, &relays) {
                    Ok(items) => {
                        if !json {
                            for i in &items {
                                println!("{}\t{}\t{}\t{}", x.title(), i.kind.label(), i.name, i.about);
                            }
                        }
                        out.push(serde_json::json!({ "exchange": x.id, "peer": x.title(), "items": items }));
                    }
                    Err(e) => {
                        ctx.io.warn(format!("{}: {e}", x.title()));
                        out.push(serde_json::json!({ "exchange": x.id, "peer": x.title(), "error": e.to_string() }));
                    }
                }
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else if xs.is_empty() {
                ctx.io.info("no key exchanges yet (`mnnn net exchange create`)");
            }
            Ok(())
        }
        NetCommand::Fetch { exchange, item, format, out, import } => {
            let mut n = Net::open(ctx)?;
            let relays = n.relays();
            let x = client::exchange_for(&n.store, &exchange)?;
            n.node()?;
            let it = client::fetch(&n.store, n.node.as_ref().expect("node"), &x, &item, format.as_deref(), &relays, ORIGIN)?;
            if import {
                let made = client::import_item(&mut n.store, &it, &x.title(), ORIGIN)?;
                ctx.io.info(format!("imported {made}"));
            }
            let binary = std::str::from_utf8(&it.data).is_err() || matches!(it.format.as_str(), "pdf" | "docx" | "mnnp");
            match out {
                Some(p) => {
                    std::fs::write(&p, &it.data).with_context(|| p.display().to_string())?;
                    ctx.io.info(format!("wrote {} ({} bytes)", p.display(), it.data.len()));
                }
                None if import => {}
                None if binary => {
                    let p = std::path::PathBuf::from(format!("{}.{}", sanitize(&it.name), it.format));
                    std::fs::write(&p, &it.data).with_context(|| p.display().to_string())?;
                    ctx.io.info(format!("wrote {} ({} bytes)", p.display(), it.data.len()));
                }
                None => println!("{}", String::from_utf8_lossy(&it.data)),
            }
            Ok(())
        }
        NetCommand::Search { spec, saved, only, remote_only, limit, json } => {
            let mut n = Net::open(ctx)?;
            let relays = n.relays();
            let sp = match saved {
                Some(s) => n.store.find_live_search(&s)?.spec,
                None => spec.apply(ctx, &n.store, SearchSpec::default())?,
            };
            n.node()?;
            let res = client::search(&n.store, n.node.as_ref(), &sp, &only, !remote_only, limit, &relays, ORIGIN)?;
            print_network_results(ctx, &res, json)
        }
        NetCommand::Shard(c) => shard(ctx, c),
    }
}

/// Results of a network search, each line starting with where it came from.
pub(super) fn print_network_results(ctx: &Ctx, res: &[client::Origin], json: bool) -> anyhow::Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(res)?);
        return Ok(());
    }
    for o in res {
        if let Some(e) = &o.error {
            ctx.io.warn(format!("{}: {e}", o.origin));
        }
        for f in &o.found {
            println!("{}\t{}", o.origin, f.line());
        }
        if o.truncated {
            ctx.io.note(format!("{}: more results than the limit", o.origin));
        }
    }
    let total: usize = res.iter().map(|o| o.found.len()).sum();
    ctx.io.note(format!("{total} match(es) from {} node(s)", res.iter().filter(|o| o.error.is_none()).count()));
    Ok(())
}

/// `mnnn search run NAME --network`.
pub fn search_network(ctx: &Ctx, search: &str, json: bool) -> anyhow::Result<()> {
    let mut n = Net::open(ctx)?;
    let relays = n.relays();
    let sp = n.store.find_live_search(search)?.spec;
    n.node()?;
    let res = client::search(&n.store, n.node.as_ref(), &sp, &[], true, 200, &relays, ORIGIN)?;
    print_network_results(ctx, &res, json)
}

pub(super) fn sanitize(name: &str) -> String {
    name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

fn start(ctx: &Ctx, port: Option<u16>, listen: Vec<String>, no_mdns: bool, relay_server: bool, for_secs: Option<u64>) -> anyhow::Result<()> {
    let n = Net::open(ctx)?;
    let mut p = n.prefs.net.clone();
    if let Some(port) = port {
        p.port = port;
    }
    if !listen.is_empty() {
        p.listen = listen;
    }
    if no_mdns {
        p.mdns = false;
    }
    if relay_server {
        p.relay_server = true;
    }
    let home = ctx.paths.home.clone();
    let mut h = local::Hosted::start(&home, &n.store, n.second_store()?, &n.kp, &n.label, &p, ORIGIN)?.settle_with(n.second_store()?, n.relays());
    println!("node {} running as {}", n.label, h.node.peer_id());
    for a in h.node.reachable_addrs() {
        println!("  {a}");
    }
    // the web server comes up with the node when `hosting.autostart` is on
    let site = if n.prefs.hosting.autostart && host::running(&home).is_none() {
        let spare = n.second_store()?;
        match host::HostOptions::from_prefs(&n.prefs.hosting).and_then(|o| host::Host::start(spare, o)) {
            Ok(site) => {
                let st = site.status();
                host::write_status(&home, &st)?;
                n.store
                    .note_action(&NewAction::new("org.host.start", ORIGIN, format!("site served at {}", st.url)).params(
                        serde_json::json!({ "addr": st.addr, "orgs": st.orgs, "mirrors": st.mirrors, "cert": st.cert.source, "auto": true }),
                    ));
                println!("site at {}", st.url);
                Some(site)
            }
            Err(e) => {
                ctx.io.warn(format!("site not started: {e}"));
                None
            }
        }
    } else {
        None
    };
    ctx.io.note("Ctrl-C (or `mnnn net stop`) stops it.");
    let stop = Arc::new(AtomicBool::new(false));
    let s2 = stop.clone();
    let _ = ctrlc::set_handler(move || s2.store(true, Ordering::SeqCst));
    let deadline = for_secs.map(|s| Instant::now() + Duration::from_secs(s));
    let mut site = site;
    let mut last_site = Instant::now();
    while !stop.load(Ordering::SeqCst) && deadline.is_none_or(|d| Instant::now() < d) && h.tick(&n.store) {
        if let Some(st) = &site
            && last_site.elapsed() >= Duration::from_secs(5)
        {
            let _ = host::write_status(&home, &st.status());
            last_site = Instant::now();
        }
        if site.is_some() && host::stop_requested(&home) {
            stop_site(&n.store, &home, site.take());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    stop_site(&n.store, &home, site);
    h.stop(&n.store, ORIGIN);
    ctx.io.note("node stopped");
    Ok(())
}

/// One line of status, a warning if the node is not the one we know by that name.
fn print_link(ctx: &Ctx, l: &auto::Link) {
    if let Some(w) = &l.warning {
        ctx.io.warn(w.clone());
    }
    ctx.io.info(format!("{}: {}", l.status.as_str(), l.detail));
}

fn stop_site(store: &Store, home: &std::path::Path, site: Option<host::Host>) {
    if let Some(h) = site {
        h.stop();
        host::clear(home);
        store.note_action(&NewAction::new("org.host.stop", ORIGIN, "site stopped"));
    }
}

fn relay(ctx: &Ctx, cmd: NetRelayCommand) -> anyhow::Result<()> {
    match cmd {
        NetRelayCommand::List { json } => {
            let prefs = ctx.prefs()?;
            let n = Net::open(ctx)?;
            let seen: Vec<_> = n.store.net_peers()?.into_iter().filter(|p| p.is_relay()).collect();
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &serde_json::json!({ "configured": prefs.net.relays, "seen": seen, "serving": prefs.net.relay_server })
                    )?
                );
            } else {
                for r in &prefs.net.relays {
                    println!("configured\t{r}");
                }
                for p in &seen {
                    println!(
                        "seen\t{}\t{}\t{}",
                        p.short_id(),
                        if p.label.is_empty() { "-" } else { &p.label },
                        p.addrs.first().map(String::as_str).unwrap_or("-")
                    );
                }
                println!("this node {}", if prefs.net.relay_server { "relays for others" } else { "does not relay (`mnnn net relay serve on`)" });
            }
            Ok(())
        }
        NetRelayCommand::Add { addr } => {
            let a: libp2p::Multiaddr = addr.parse().map_err(|e| anyhow!("{addr}: {e}"))?;
            if net::node::addr_peer(&a).is_none() {
                return Err(anyhow!("a relay address needs its peer id at the end (…/p2p/12D3Koo…)"));
            }
            let mut prefs = ctx.prefs()?;
            if !prefs.net.relays.contains(&addr) {
                prefs.net.relays.push(addr.clone());
                prefs.save(&ctx.paths)?;
            }
            ctx.io.info(format!("using relay {addr} (from the next node start)"));
            Ok(())
        }
        NetRelayCommand::Remove { relay } => {
            let mut prefs = ctx.prefs()?;
            let before = prefs.net.relays.len();
            prefs.net.relays.retain(|r| r != &relay && !r.ends_with(&relay) && !r.contains(&format!("/p2p/{relay}")));
            if prefs.net.relays.len() == before {
                return Err(anyhow!("no configured relay matches `{relay}`"));
            }
            prefs.save(&ctx.paths)?;
            ctx.io.info("relay removed");
            Ok(())
        }
        NetRelayCommand::Serve { state } => {
            let mut prefs = ctx.prefs()?;
            prefs.net.relay_server = state.on();
            prefs.save(&ctx.paths)?;
            ctx.io.info(if state.on() { "this node will relay for others (from the next start)" } else { "this node will not relay" });
            Ok(())
        }
    }
}

fn print_exchange(n: &Net, x: &crate::store::Exchange, tokens: bool) {
    println!("id:          {}", x.id);
    println!("label:       {}", if x.label.is_empty() { "-" } else { &x.label });
    println!("status:      {}", x.status.as_str());
    println!("role:        {}", x.role);
    println!("peer:        {} {}", if x.peer_label.is_empty() { "-" } else { &x.peer_label }, x.peer_id.as_deref().unwrap_or(""));
    if let Some(c) = &x.short_code {
        println!(
            "short code:  {c}{}",
            if x.status == ExchangeStatus::Verified { " (verified)" } else { " (compare with the other side, then `verify`)" }
        );
    }
    if let Some(f) = &x.fingerprint {
        println!("fingerprint: {f}");
    }
    if let Some(e) = x.expires_at
        && x.status == ExchangeStatus::Offered
    {
        println!("expires:     {}", fmt_ts(e));
    }
    if let Some(r) = x.revoked_at {
        println!("revoked:     {} by {}", fmt_ts(r), x.revoked_by.as_deref().unwrap_or("?"));
    }
    println!("author:      {}", crate::history::author_label(&n.store, x.author_id.as_deref()));
    println!("created:     {}", fmt_ts(x.created_at));
    if let Ok(sh) = n.store.shares_for(&x.id) {
        println!("we share:    {} item(s)", sh.len());
    }
    if tokens {
        println!("offer:       {}", x.offer);
        if let Some(a) = &x.answer {
            println!("answer:      {a}");
        }
    }
}

fn exchange(ctx: &Ctx, cmd: NetExchangeCommand) -> anyhow::Result<()> {
    match cmd {
        NetExchangeCommand::Create { label, peer, hours, json } => {
            let mut n = Net::open(ctx)?;
            let relays = n.relays();
            let addrs = n.addrs();
            if peer.is_some() {
                n.node()?;
            }
            let c = client::create(
                &n.store,
                n.node.as_ref(),
                &n.kp,
                &label,
                &n.label,
                addrs,
                peer.as_deref(),
                (hours as i64) * 3_600_000,
                &relays,
                ORIGIN,
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&c)?);
                return Ok(());
            }
            match &c.delivered {
                Some(Ok(_)) => {
                    ctx.io.info(format!("offer {} sent; the other side accepts it with `mnnn net exchange accept`", c.exchange.short_id()))
                }
                Some(Err(e)) => ctx.io.warn(format!("could not send the offer ({e}); pass this token on instead")),
                None => {}
            }
            if !matches!(c.delivered, Some(Ok(_))) {
                ctx.io.note("Give this offer to the other side (they run `mnnn net exchange accept TOKEN`):");
                println!("{}", c.token);
            }
            Ok(())
        }
        NetExchangeCommand::Accept { input, label, json } => {
            let mut n = Net::open(ctx)?;
            let token = read_token(&input)?;
            let addrs = n.addrs();
            n.node()?;
            let a = client::accept(&n.store, n.node.as_ref(), &n.kp, &token, &label, &n.label, addrs, ORIGIN)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&a)?);
                return Ok(());
            }
            let x = &a.exchange;
            ctx.io.info(format!("key exchange with {} complete", x.title()));
            if let Some(c) = &x.short_code {
                println!("short code: {c}");
                ctx.io.note("Read the code to each other (call, in person). If it matches, run `mnnn net exchange verify` with it.");
            }
            if let Some(ans) = &a.answer
                && !matches!(a.delivered, Some(Ok(_)))
            {
                if let Some(Err(e)) = &a.delivered {
                    ctx.io.warn(format!("could not reach the offerer ({e})"));
                }
                ctx.io.note("Give this answer back (they run `mnnn net exchange accept TOKEN`):");
                println!("{ans}");
            }
            Ok(())
        }
        NetExchangeCommand::List { json } => {
            let n = Net::open(ctx)?;
            let xs = n.store.exchanges()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&xs)?);
            } else {
                for x in &xs {
                    println!(
                        "{}\t{}\t{}\t{}\t{}",
                        x.short_id(),
                        x.title(),
                        x.status.as_str(),
                        x.short_code.as_deref().unwrap_or("-"),
                        x.peer_id.as_deref().map(crate::store::net::short_peer).unwrap_or("-")
                    );
                }
                if xs.is_empty() {
                    ctx.io.info("no key exchanges yet (`mnnn net exchange create`)");
                }
            }
            Ok(())
        }
        NetExchangeCommand::Show { exchange, tokens, json } => {
            let n = Net::open(ctx)?;
            let x = client::exchange_for(&n.store, &exchange)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&x)?);
            } else {
                print_exchange(&n, &x, tokens);
            }
            Ok(())
        }
        NetExchangeCommand::Verify { exchange, code } => {
            let n = Net::open(ctx)?;
            let x = client::exchange_for(&n.store, &exchange)?;
            let x = client::verify(&n.store, &x.id, &code, ORIGIN)?;
            ctx.io.info(format!("key exchange with {} verified", x.title()));
            Ok(())
        }
        NetExchangeCommand::Revoke { exchange } => {
            let mut n = Net::open(ctx)?;
            let relays = n.relays();
            let x = client::exchange_for(&n.store, &exchange)?;
            if !ctx.io.confirm(&format!("Revoke the key exchange with {}? Its key is erased and sharing over it stops.", x.title()))? {
                return Err(anyhow!("cancelled"));
            }
            if x.status.active() {
                let _ = n.node();
            }
            let (x, told) = client::revoke(&n.store, n.node.as_ref(), &x.id, &relays, ORIGIN)?;
            ctx.io.info(format!(
                "key exchange with {} revoked{}",
                x.title(),
                match told {
                    Some(Ok(())) => " (peer told)",
                    Some(Err(_)) => " (peer not reachable; it will be refused next time)",
                    None => "",
                }
            ));
            Ok(())
        }
        NetExchangeCommand::Label { exchange, label } => {
            let n = Net::open(ctx)?;
            let x = client::exchange_for(&n.store, &exchange)?;
            n.store.label_exchange(&x.id, &label)?;
            ctx.io.info(format!("exchange {} is now {label}", x.short_id()));
            Ok(())
        }
        NetExchangeCommand::Export { out } => {
            let n = Net::open(ctx)?;
            let pw = ctx.export_password("Backup password")?;
            let bytes = net::backup::export(&n.store, &pw)?;
            crate::paths::write_private(&out, &bytes)?;
            ctx.io.info(format!("wrote {} ({} exchange(s), age-encrypted)", out.display(), n.store.exchanges()?.len()));
            Ok(())
        }
        NetExchangeCommand::Restore { file } => {
            let n = Net::open(ctx)?;
            let bytes = std::fs::read(&file).with_context(|| file.display().to_string())?;
            let pw = ctx.export_password("Backup password")?;
            let (peer, count, replaces) = net::backup::peek(&n.store, &bytes, &pw)?;
            if replaces
                && !ctx.io.confirm(&format!(
                    "This database's node identity becomes {} (from the backup). Exchanges made with the current identity stop working. Continue?",
                    crate::store::net::short_peer(&peer)
                ))?
            {
                return Err(anyhow!("cancelled"));
            }
            let r = net::backup::restore(&n.store, &bytes, &pw, ORIGIN)?;
            ctx.io.info(format!("restored {count} key exchange(s) and {} peer(s) for node {}", r.peers, crate::store::net::short_peer(&r.peer_id)));
            Ok(())
        }
    }
}

fn shard(ctx: &Ctx, cmd: NetShardCommand) -> anyhow::Result<()> {
    match cmd {
        NetShardCommand::Entrust { to, threshold, master } => {
            let mut n = Net::open(ctx)?;
            let relays = n.relays();
            let unverified: Vec<String> = to
                .iter()
                .filter_map(|k| client::exchange_for(&n.store, k).ok())
                .filter(|x| x.status != ExchangeStatus::Verified)
                .map(|x| x.title())
                .collect();
            if !unverified.is_empty()
                && !ctx.io.confirm(&format!("The short codes of {} were never verified. Entrust shards anyway?", unverified.join(", ")))?
            {
                return Err(anyhow!("cancelled: verify the exchanges first (`mnnn net exchange verify`)"));
            }
            let key = super::keys::master(ctx, &master)?;
            let pw = shard_password(ctx)?;
            n.node()?;
            let e = client::entrust(&n.store, n.node.as_ref().expect("node"), &key, &pw, &to, threshold, &relays, ORIGIN)?;
            ctx.io.info(format!("dark shards: any {} of {} restore the master key (set {})", e.threshold, e.placed.len(), e.set));
            for p in &e.placed {
                let x = n.store.exchange(&p.exchange_id)?.map(|x| x.title()).unwrap_or_default();
                ctx.io.info(format!("  shard {} → {x}", p.index));
            }
            ctx.io.note("Keep the shard password: it is needed (with enough peers) to restore.");
            Ok(())
        }
        NetShardCommand::List { json } => {
            let n = Net::open(ctx)?;
            let mine = n.store.entrusted()?;
            let held = n.store.held_shards()?;
            if json {
                let held: Vec<_> = held
                    .iter()
                    .map(|h| serde_json::json!({ "id": h.id, "owner": h.owner_peer, "owner_label": h.owner_label, "set": h.set_id, "index": h.index, "threshold": h.threshold, "total": h.total, "exchange": h.exchange_id, "stored_at": h.stored_at }))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "entrusted": mine, "held": held }))?);
            } else {
                for e in &mine {
                    let x = n.store.exchange(&e.exchange_id)?.map(|x| format!("{} ({})", x.title(), x.status.as_str())).unwrap_or_default();
                    println!("entrusted\t{}\t{}/{} of {}\t{x}", e.set_id, e.index, e.threshold, e.total);
                }
                for h in &held {
                    println!(
                        "held\t{}\t{}\t{}\tfor {}",
                        crate::store::projects::short_of(&h.id),
                        h.set_id,
                        h.index,
                        if h.owner_label.is_empty() { &h.owner_peer } else { &h.owner_label }
                    );
                }
                if mine.is_empty() && held.is_empty() {
                    ctx.io.info("no dark shards on the network; `mnnn net shard entrust --to A --to B --to C`");
                }
            }
            Ok(())
        }
        NetShardCommand::Restore { backup, set, from, recover, print_words } => {
            let pw = shard_password(ctx)?;
            let prefs = ctx.prefs()?;
            let (key, g) = match &backup {
                Some(f) => {
                    let bytes = std::fs::read(f).with_context(|| f.display().to_string())?;
                    let bpw = ctx.export_password("Backup password")?;
                    let scratch = net::backup::scratch(&bytes, &bpw)?;
                    let kp = identity::load_or_create(&scratch)?;
                    let label = local::my_label(&scratch, &prefs.net);
                    let svc = net::backup::scratch(&bytes, &bpw)?;
                    let node = local::ephemeral(&kp, svc, &label, &prefs.net)?;
                    client::gather(&scratch, &node, &pw, set.as_deref(), &from, &prefs.net.relays, ORIGIN)?
                }
                None => {
                    let mut n = Net::open(ctx)?;
                    let relays = n.relays();
                    n.node()?;
                    client::gather(&n.store, n.node.as_ref().expect("node"), &pw, set.as_deref(), &from, &relays, ORIGIN)?
                }
            };
            keyring::verify_master(&ctx.paths, &key).map_err(|e| anyhow!("the shards rebuilt a key that does not open this database ({e})"))?;
            ctx.io.info(format!("master key restored from {} shard(s) (set {}) held by {}", g.got, g.set, g.from.join(", ")));
            if recover || backup.is_some() {
                let a = super::keys::new_password(ctx, "New Muninn password")?;
                keyring::recover(&ctx.paths, &key, &a)?;
                ctx.io.info("new Muninn password set");
            }
            if print_words {
                let w = crypto::to_words(&key)?;
                println!("{}", w.as_str());
            }
            Ok(())
        }
        NetShardCommand::Drop { id } => {
            let n = Net::open(ctx)?;
            let held = n.store.held_shards()?;
            let h = held.iter().find(|h| h.id == id || h.id.starts_with(&id)).ok_or_else(|| anyhow!("no held shard `{id}`"))?;
            if !ctx.io.confirm(&format!(
                "Drop the shard you hold for {}? They may need it to restore their key.",
                if h.owner_label.is_empty() { &h.owner_peer } else { &h.owner_label }
            ))? {
                return Err(anyhow!("cancelled"));
            }
            let (owner, set) = (h.owner_peer.clone(), h.set_id.clone());
            n.store.drop_held_shard(&id)?;
            n.store.note_action(
                &NewAction::new("net.shard.drop", ORIGIN, "a held dark shard dropped").params(serde_json::json!({ "peer": owner, "set": set })),
            );
            ctx.io.info("dropped");
            Ok(())
        }
    }
}
