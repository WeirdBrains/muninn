//! The libp2p node: TCP and QUIC (or an in-memory transport for tests) with Noise and
//! Yamux; mDNS on the local network; Kademlia, identify and ping; relay client and
//! (opt-in) relay server with DCUtR hole punching; AutoNAT; and request-response on
//! [`PROTOCOL`](super::proto::PROTOCOL).
//!
//! The swarm runs on its own thread with a small Tokio runtime. [`Node`] is a blocking
//! handle (the CLI and TUI are synchronous): requests wait for their answer. Requests
//! that come in are answered by a [`Handler`] on Tokio's blocking pool.
use super::proto::{KAD_PROTOCOL, PROTOCOL, RELAY_KEY, Wire, WireResp};
use crate::error::{Error, Result};
use futures::StreamExt;
use libp2p::core::muxing::StreamMuxerBox;
use libp2p::core::transport::{Boxed, MemoryTransport, Transport};
use libp2p::core::upgrade::Version;
use libp2p::identity::Keypair;
use libp2p::kad::{self, store::MemoryStore};
use libp2p::multiaddr::Protocol;
use libp2p::request_response::{self, OutboundRequestId, ProtocolSupport};
use libp2p::swarm::behaviour::toggle::Toggle;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{Multiaddr, PeerId, StreamProtocol, Swarm, autonat, dcutr, identify, mdns, noise, ping, relay, tcp, yamux};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

/// The default port (TCP and QUIC).
pub const DEFAULT_PORT: u16 = 4741;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const REQUEST_MAX: u64 = 8 * 1024 * 1024;
const RESPONSE_MAX: u64 = 256 * 1024 * 1024;

/// How a node runs.
#[derive(Debug, Clone, Default)]
pub struct NodeConfig {
    /// Addresses to listen on (none: only dial out).
    pub listen: Vec<Multiaddr>,
    /// Find and announce peers on the local network.
    pub mdns: bool,
    /// Relay traffic for other nodes (and say so).
    pub relay_server: bool,
    /// Kademlia bootstrap nodes (`/ip4/…/tcp/…/p2p/…`).
    pub bootstrap: Vec<Multiaddr>,
    /// Relays to reserve a slot on (`/…/p2p/RELAY`); reachable through them behind NAT.
    pub relays: Vec<Multiaddr>,
    /// The in-memory transport (tests) instead of TCP and QUIC.
    pub memory: bool,
    /// This node's label (in identify; empty: none).
    pub label: String,
    /// Addresses others reach us at from outside (a DNS name or public IP forwarded to
    /// this node), announced and put in invites before the local ones.
    pub external: Vec<Multiaddr>,
}

impl NodeConfig {
    /// TCP and QUIC on every interface, IPv4 and IPv6, on `port`.
    pub fn default_listen(port: u16) -> Vec<Multiaddr> {
        [
            format!("/ip4/0.0.0.0/tcp/{port}"),
            format!("/ip4/0.0.0.0/udp/{port}/quic-v1"),
            format!("/ip6/::/tcp/{port}"),
            format!("/ip6/::/udp/{port}/quic-v1"),
        ]
        .iter()
        .filter_map(|a| a.parse().ok())
        .collect()
    }
}

/// A peer seen by the node, for the handler to remember.
#[derive(Debug, Clone)]
pub struct Sighting {
    pub peer: PeerId,
    pub addrs: Vec<String>,
    pub caps: Option<Vec<String>>,
    pub agent: Option<String>,
    pub source: &'static str,
}

/// Answers requests from peers.
pub trait Handler: Send + Sync + 'static {
    fn request(&self, from: &PeerId, req: Wire) -> WireResp;
    /// A peer was discovered or identified.
    fn seen(&self, _s: Sighting) {}
}

/// What the node is doing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub peer_id: String,
    pub listening: Vec<String>,
    pub external: Vec<String>,
    pub connected: Vec<String>,
    /// AutoNAT: `public`, `private` or `unknown`.
    pub nat: String,
    /// Relays holding a reservation for us.
    pub reservations: Vec<String>,
    pub relay_server: bool,
    pub mdns: bool,
    pub started_at: i64,
}

#[derive(NetworkBehaviour)]
struct Behaviour {
    rr: request_response::cbor::Behaviour<Wire, WireResp>,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    kad: kad::Behaviour<MemoryStore>,
    mdns: Toggle<mdns::tokio::Behaviour>,
    relay_client: relay::client::Behaviour,
    relay_server: Toggle<relay::Behaviour>,
    dcutr: dcutr::Behaviour,
    autonat: autonat::Behaviour,
}

enum Cmd {
    Request { peer: PeerId, addrs: Vec<Multiaddr>, wire: Wire, reply: oneshot::Sender<Result<WireResp>> },
    Dial { addr: Multiaddr, reply: oneshot::Sender<Result<PeerId>> },
    Listen { addr: Multiaddr, reply: oneshot::Sender<Result<Multiaddr>> },
    AddRelay { addr: Multiaddr },
    Stop,
}

/// A running node.
pub struct Node {
    peer_id: PeerId,
    tx: mpsc::UnboundedSender<Cmd>,
    status: Arc<Mutex<Status>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

fn net_err(e: impl std::fmt::Display) -> Error {
    Error::Other(format!("network: {e}"))
}

fn behaviour(key: &Keypair, relay_client: relay::client::Behaviour, cfg: &NodeConfig) -> Result<Behaviour> {
    let peer = key.public().to_peer_id();
    let rr = request_response::cbor::Behaviour::with_codec(
        request_response::cbor::codec::Codec::default().set_request_size_maximum(REQUEST_MAX).set_response_size_maximum(RESPONSE_MAX),
        [(StreamProtocol::new(PROTOCOL), ProtocolSupport::Full)],
        request_response::Config::default().with_request_timeout(REQUEST_TIMEOUT),
    );
    let mut agent = format!("muninn/{}", env!("CARGO_PKG_VERSION"));
    if !cfg.label.trim().is_empty() {
        agent += &format!(" ({})", cfg.label.trim());
    }
    let identify = identify::Behaviour::new(identify::Config::new("/muninn/id/1".into(), key.public()).with_agent_version(agent));
    let mut kcfg = kad::Config::new(StreamProtocol::new(KAD_PROTOCOL));
    kcfg.set_query_timeout(Duration::from_secs(30));
    let kad = kad::Behaviour::with_config(peer, MemoryStore::new(peer), kcfg);
    let mdns = if cfg.mdns && !cfg.memory { Some(mdns::tokio::Behaviour::new(mdns::Config::default(), peer).map_err(net_err)?) } else { None };
    let relay_server = cfg.relay_server.then(|| relay::Behaviour::new(peer, relay::Config::default()));
    let autonat = autonat::Behaviour::new(peer, autonat::Config { only_global_ips: !cfg.memory, ..Default::default() });
    Ok(Behaviour {
        rr,
        identify,
        ping: ping::Behaviour::new(ping::Config::new()),
        kad,
        mdns: mdns.into(),
        relay_client,
        relay_server: relay_server.into(),
        dcutr: dcutr::Behaviour::new(peer),
        autonat,
    })
}

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

fn memory_transport(key: &Keypair) -> std::result::Result<Boxed<(PeerId, StreamMuxerBox)>, BoxErr> {
    Ok(MemoryTransport::default()
        .upgrade(Version::V1)
        .authenticate(noise::Config::new(key).map_err(|e| Box::new(e) as BoxErr)?)
        .multiplex(yamux::Config::default())
        .timeout(Duration::from_secs(20))
        .boxed())
}

fn build_swarm(key: Keypair, cfg: &NodeConfig) -> Result<Swarm<Behaviour>> {
    let idle = |c: libp2p::swarm::Config| c.with_idle_connection_timeout(Duration::from_secs(120));
    let swarm = if cfg.memory {
        let cfg2 = cfg.clone();
        libp2p::SwarmBuilder::with_existing_identity(key)
            .with_tokio()
            .with_other_transport(memory_transport)
            .map_err(net_err)?
            .with_relay_client(noise::Config::new, yamux::Config::default)
            .map_err(net_err)?
            .with_behaviour(move |key, relay| behaviour(key, relay, &cfg2).map_err(|e| Box::new(e) as BoxErr))
            .map_err(net_err)?
            .with_swarm_config(idle)
            .build()
    } else {
        let cfg2 = cfg.clone();
        libp2p::SwarmBuilder::with_existing_identity(key)
            .with_tokio()
            .with_tcp(tcp::Config::default().nodelay(true), noise::Config::new, yamux::Config::default)
            .map_err(net_err)?
            .with_quic()
            .with_dns()
            .map_err(net_err)?
            .with_relay_client(noise::Config::new, yamux::Config::default)
            .map_err(net_err)?
            .with_behaviour(move |key, relay| behaviour(key, relay, &cfg2).map_err(|e| Box::new(e) as BoxErr))
            .map_err(net_err)?
            .with_swarm_config(idle)
            .build()
    };
    Ok(swarm)
}

/// The peer id at the end of an address (`…/p2p/ID`).
pub fn addr_peer(a: &Multiaddr) -> Option<PeerId> {
    a.iter().filter_map(|p| if let Protocol::P2p(id) = p { Some(id) } else { None }).last()
}

/// Parse addresses, skipping bad ones.
pub fn parse_addrs<S: AsRef<str>>(v: &[S]) -> Vec<Multiaddr> {
    v.iter().filter_map(|s| s.as_ref().trim().parse().ok()).collect()
}

/// Whether an address is worth giving to others (not unspecified or loopback-only wildcard).
pub fn shareable(a: &Multiaddr) -> bool {
    !a.iter().any(|p| match p {
        Protocol::Ip4(ip) => ip.is_unspecified(),
        Protocol::Ip6(ip) => ip.is_unspecified(),
        _ => false,
    })
}

impl Node {
    /// Start a node; returns once its listeners are up (or failed).
    pub fn start(key: Keypair, cfg: NodeConfig, handler: Arc<dyn Handler>) -> Result<Node> {
        let peer_id = key.public().to_peer_id();
        let status = Arc::new(Mutex::new(Status {
            peer_id: peer_id.to_string(),
            nat: "unknown".into(),
            relay_server: cfg.relay_server,
            mdns: cfg.mdns && !cfg.memory,
            started_at: crate::store::now_ms(),
            ..Default::default()
        }));
        let (tx, rx) = mpsc::unbounded_channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<()>>();
        let st = status.clone();
        let thread = std::thread::Builder::new()
            .name("muninn-net".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().thread_name("muninn-net-rt").build() {
                    Ok(rt) => rt,
                    Err(e) => {
                        let _ = ready_tx.send(Err(net_err(e)));
                        return;
                    }
                };
                rt.block_on(async move {
                    let swarm = match build_swarm(key, &cfg) {
                        Ok(mut s) => {
                            for a in &cfg.external {
                                s.add_external_address(a.clone());
                            }
                            s
                        }
                        Err(e) => {
                            let _ = ready_tx.send(Err(e));
                            return;
                        }
                    };
                    let mut l = Loop::new(swarm, cfg, handler, st);
                    l.run(rx, ready_tx).await;
                });
                rt.shutdown_timeout(Duration::from_secs(2));
            })
            .map_err(net_err)?;
        match ready_rx.recv_timeout(Duration::from_secs(20)) {
            Ok(Ok(())) => Ok(Node { peer_id, tx, status, thread: Some(thread) }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err(Error::Other("network: the node did not start in time".into())),
        }
    }

    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    pub fn status(&self) -> Status {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Addresses others can use to reach this node (with `/p2p/ID`).
    pub fn reachable_addrs(&self) -> Vec<String> {
        let s = self.status();
        let mut v: Vec<String> = vec![];
        for a in s.external.iter().chain(s.listening.iter()) {
            if let Ok(m) = a.parse::<Multiaddr>()
                && shareable(&m)
            {
                let full = if addr_peer(&m).is_some() { m } else { m.with(Protocol::P2p(self.peer_id)) };
                let t = full.to_string();
                if !v.contains(&t) {
                    v.push(t);
                }
            }
        }
        v
    }

    fn call<T>(&self, make: impl FnOnce(oneshot::Sender<Result<T>>) -> Cmd, wait: Duration) -> Result<T> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(make(tx)).map_err(|_| Error::Other("network: the node has stopped".into()))?;
        let deadline = std::time::Instant::now() + wait;
        let mut rx = rx;
        loop {
            match rx.try_recv() {
                Ok(r) => return r,
                Err(oneshot::error::TryRecvError::Closed) => return Err(Error::Other("network: the node has stopped".into())),
                Err(oneshot::error::TryRecvError::Empty) => {
                    if std::time::Instant::now() > deadline {
                        return Err(Error::Other("network: timed out".into()));
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        }
    }

    /// Send a request and wait for the answer. `addrs` are tried if not connected yet.
    pub fn request(&self, peer: PeerId, addrs: &[Multiaddr], wire: Wire) -> Result<WireResp> {
        let addrs = addrs.to_vec();
        self.call(|reply| Cmd::Request { peer, addrs, wire, reply }, REQUEST_TIMEOUT + Duration::from_secs(5))
    }

    /// Connect to an address; returns the peer there.
    pub fn dial(&self, addr: Multiaddr) -> Result<PeerId> {
        self.call(|reply| Cmd::Dial { addr, reply }, Duration::from_secs(30))
    }

    /// Listen on one more address; returns the address it got.
    pub fn listen(&self, addr: Multiaddr) -> Result<Multiaddr> {
        self.call(|reply| Cmd::Listen { addr, reply }, Duration::from_secs(20))
    }

    /// Reserve a slot on a relay (`…/p2p/RELAY`).
    pub fn add_relay(&self, addr: Multiaddr) -> Result<()> {
        self.tx.send(Cmd::AddRelay { addr }).map_err(|_| Error::Other("network: the node has stopped".into()))
    }

    /// Whether the node thread still runs.
    pub fn running(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| !t.is_finished())
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    /// Ask the node to stop without waiting (others may still hold it).
    pub fn halt(&self) {
        let _ = self.tx.send(Cmd::Stop);
    }

    fn shutdown(&mut self) {
        let _ = self.tx.send(Cmd::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct Loop {
    swarm: Swarm<Behaviour>,
    cfg: NodeConfig,
    handler: Arc<dyn Handler>,
    status: Arc<Mutex<Status>>,
    pending: HashMap<OutboundRequestId, oneshot::Sender<Result<WireResp>>>,
    dials: Vec<(Multiaddr, Option<PeerId>, oneshot::Sender<Result<PeerId>>)>,
    listens: HashMap<libp2p::core::transport::ListenerId, oneshot::Sender<Result<Multiaddr>>>,
    relays: HashSet<PeerId>,
    relay_listening: HashSet<PeerId>,
    resp_tx: mpsc::UnboundedSender<(request_response::ResponseChannel<WireResp>, WireResp)>,
    resp_rx: mpsc::UnboundedReceiver<(request_response::ResponseChannel<WireResp>, WireResp)>,
}

impl Loop {
    fn new(swarm: Swarm<Behaviour>, cfg: NodeConfig, handler: Arc<dyn Handler>, status: Arc<Mutex<Status>>) -> Self {
        let (resp_tx, resp_rx) = mpsc::unbounded_channel();
        Loop {
            swarm,
            cfg,
            handler,
            status,
            pending: HashMap::new(),
            dials: vec![],
            listens: HashMap::new(),
            relays: HashSet::new(),
            relay_listening: HashSet::new(),
            resp_tx,
            resp_rx,
        }
    }

    fn update(&self, f: impl FnOnce(&mut Status)) {
        if let Ok(mut s) = self.status.lock() {
            f(&mut s);
        }
    }

    fn refresh(&self) {
        let connected: Vec<String> = self.swarm.connected_peers().map(|p| p.to_string()).collect();
        let listening: Vec<String> = self.swarm.listeners().map(|a| a.to_string()).collect();
        let external: Vec<String> = self.swarm.external_addresses().map(|a| a.to_string()).collect();
        self.update(|s| {
            s.connected = connected;
            s.listening = listening;
            s.external = external;
        });
    }

    fn seen(&self, s: Sighting) {
        let h = self.handler.clone();
        tokio::task::spawn_blocking(move || h.seen(s));
    }

    async fn run(&mut self, mut rx: mpsc::UnboundedReceiver<Cmd>, ready: std::sync::mpsc::Sender<Result<()>>) {
        // listen
        let mut waiting = 0usize;
        let mut ready = Some(ready);
        for a in self.cfg.listen.clone() {
            match self.swarm.listen_on(a.clone()) {
                Ok(_) => waiting += 1,
                Err(e) => tracing::warn!("network: cannot listen on {a}: {e}"),
            }
        }
        if !self.cfg.listen.is_empty() && waiting == 0 {
            if let Some(r) = ready.take() {
                let _ = r.send(Err(Error::Other("network: could not listen on any address".into())));
            }
            return;
        }
        for b in self.cfg.bootstrap.clone() {
            if let Some(p) = addr_peer(&b) {
                self.swarm.behaviour_mut().kad.add_address(&p, b.clone());
                let _ = self.swarm.dial(b);
            }
        }
        for r in self.cfg.relays.clone() {
            self.add_relay(r);
        }
        if self.cfg.relay_server {
            self.swarm.behaviour_mut().kad.set_mode(Some(kad::Mode::Server));
        }
        if waiting == 0
            && let Some(r) = ready.take()
        {
            let _ = r.send(Ok(()));
        }
        let ready_deadline = tokio::time::sleep(Duration::from_secs(5));
        tokio::pin!(ready_deadline);
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tokio::select! {
                cmd = rx.recv() => match cmd {
                    None | Some(Cmd::Stop) => break,
                    Some(c) => self.command(c),
                },
                Some((ch, resp)) = self.resp_rx.recv() => {
                    let _ = self.swarm.behaviour_mut().rr.send_response(ch, resp);
                }
                ev = self.swarm.select_next_some() => {
                    let new_listen = matches!(ev, SwarmEvent::NewListenAddr { .. });
                    self.event(ev);
                    if new_listen && waiting > 0 {
                        waiting -= 1;
                        if waiting == 0 && let Some(r) = ready.take() {
                            let _ = r.send(Ok(()));
                        }
                    }
                }
                _ = &mut ready_deadline, if ready.is_some() => {
                    if let Some(r) = ready.take() {
                        let _ = r.send(Ok(()));
                    }
                }
                _ = tick.tick() => self.periodic(),
            }
        }
        for (_, tx) in self.pending.drain() {
            let _ = tx.send(Err(Error::Other("network: the node stopped".into())));
        }
    }

    fn periodic(&mut self) {
        let k = &mut self.swarm.behaviour_mut().kad;
        if k.bootstrap().is_ok() {
            k.get_providers(kad::RecordKey::new(&RELAY_KEY));
            if self.cfg.relay_server {
                let _ = k.start_providing(kad::RecordKey::new(&RELAY_KEY));
            }
        }
    }

    fn add_relay(&mut self, addr: Multiaddr) {
        let Some(peer) = addr_peer(&addr) else {
            tracing::warn!("network: relay address {addr} has no /p2p/ peer id");
            return;
        };
        if peer == *self.swarm.local_peer_id() || !self.relay_listening.insert(peer) {
            return;
        }
        self.relays.insert(peer);
        self.swarm.add_peer_address(peer, addr.clone());
        let circuit = addr.with(Protocol::P2pCircuit);
        if let Err(e) = self.swarm.listen_on(circuit.clone()) {
            tracing::warn!("network: cannot listen through relay {circuit}: {e}");
            self.relay_listening.remove(&peer);
        }
    }

    fn command(&mut self, c: Cmd) {
        match c {
            Cmd::Request { peer, addrs, wire, reply } => {
                for a in addrs {
                    self.swarm.add_peer_address(peer, a);
                }
                let id = self.swarm.behaviour_mut().rr.send_request(&peer, wire);
                self.pending.insert(id, reply);
            }
            Cmd::Dial { addr, reply } => {
                let peer = addr_peer(&addr);
                match self.swarm.dial(addr.clone()) {
                    Ok(()) => self.dials.push((addr, peer, reply)),
                    Err(e) => {
                        let _ = reply.send(Err(net_err(e)));
                    }
                }
            }
            Cmd::Listen { addr, reply } => match self.swarm.listen_on(addr) {
                Ok(id) => {
                    self.listens.insert(id, reply);
                }
                Err(e) => {
                    let _ = reply.send(Err(net_err(e)));
                }
            },
            Cmd::AddRelay { addr } => self.add_relay(addr),
            Cmd::Stop => {}
        }
    }

    fn event(&mut self, ev: SwarmEvent<BehaviourEvent>) {
        match ev {
            SwarmEvent::NewListenAddr { listener_id, address } => {
                tracing::info!("network: listening on {address}");
                // a relay announces where it listens: reservations carry those addresses
                if self.cfg.relay_server && shareable(&address) && !address.iter().any(|p| p == Protocol::P2pCircuit) {
                    self.swarm.add_external_address(address.clone());
                }
                if let Some(tx) = self.listens.remove(&listener_id) {
                    let _ = tx.send(Ok(address));
                }
                self.refresh();
            }
            SwarmEvent::ListenerClosed { listener_id, .. } => {
                if let Some(tx) = self.listens.remove(&listener_id) {
                    let _ = tx.send(Err(Error::Other("network: listener closed".into())));
                }
                self.refresh();
            }
            SwarmEvent::ExternalAddrConfirmed { .. } | SwarmEvent::ExternalAddrExpired { .. } | SwarmEvent::ExpiredListenAddr { .. } => {
                self.refresh()
            }
            SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                let addr = endpoint.get_remote_address().clone();
                let mut i = 0;
                while i < self.dials.len() {
                    let (a, p, _) = &self.dials[i];
                    if *a == addr || p.is_some_and(|p| p == peer_id) {
                        let (_, _, tx) = self.dials.remove(i);
                        let _ = tx.send(Ok(peer_id));
                    } else {
                        i += 1;
                    }
                }
                self.refresh();
            }
            SwarmEvent::ConnectionClosed { .. } => self.refresh(),
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                let msg = error.to_string();
                let mut i = 0;
                while i < self.dials.len() {
                    if peer_id.is_some() && self.dials[i].1 == peer_id {
                        let (_, _, tx) = self.dials.remove(i);
                        let _ = tx.send(Err(net_err(&msg)));
                    } else {
                        i += 1;
                    }
                }
                if peer_id.is_none() && !self.dials.is_empty() {
                    // an address without /p2p/: the oldest dial failed
                    let (_, _, tx) = self.dials.remove(0);
                    let _ = tx.send(Err(net_err(&msg)));
                }
            }
            SwarmEvent::Behaviour(b) => self.behaviour(b),
            _ => {}
        }
    }

    fn behaviour(&mut self, ev: BehaviourEvent) {
        match ev {
            BehaviourEvent::Rr(request_response::Event::Message { peer, message, .. }) => match message {
                request_response::Message::Request { request, channel, .. } => {
                    let h = self.handler.clone();
                    let tx = self.resp_tx.clone();
                    tokio::task::spawn_blocking(move || {
                        let resp = h.request(&peer, request);
                        let _ = tx.send((channel, resp));
                    });
                }
                request_response::Message::Response { request_id, response } => {
                    if let Some(tx) = self.pending.remove(&request_id) {
                        let _ = tx.send(Ok(response));
                    }
                }
            },
            BehaviourEvent::Rr(request_response::Event::OutboundFailure { request_id, error, peer, .. }) => {
                if let Some(tx) = self.pending.remove(&request_id) {
                    let _ = tx.send(Err(Error::Other(format!("network: {} could not be reached ({error})", super::short_peer_id(&peer)))));
                }
            }
            BehaviourEvent::Rr(request_response::Event::InboundFailure { peer, error, .. }) => {
                tracing::debug!("network: inbound request from {peer} failed: {error}");
            }
            BehaviourEvent::Mdns(mdns::Event::Discovered(list)) => {
                let mut by_peer: HashMap<PeerId, Vec<Multiaddr>> = HashMap::new();
                for (p, a) in list {
                    by_peer.entry(p).or_default().push(a);
                }
                for (p, addrs) in by_peer {
                    for a in &addrs {
                        self.swarm.add_peer_address(p, a.clone());
                        self.swarm.behaviour_mut().kad.add_address(&p, a.clone());
                    }
                    if !self.swarm.is_connected(&p) {
                        let _ = self.swarm.dial(p);
                    }
                    self.seen(Sighting { peer: p, addrs: addrs.iter().map(|a| a.to_string()).collect(), caps: None, agent: None, source: "mdns" });
                }
            }
            BehaviourEvent::Identify(identify::Event::Received { peer_id, info, .. }) => {
                let protos: Vec<String> = info.protocols.iter().map(|p| p.to_string()).collect();
                let mut caps = vec![];
                if protos.iter().any(|p| p == PROTOCOL) {
                    caps.push("muninn".to_string());
                }
                if protos.iter().any(|p| *p == relay::HOP_PROTOCOL_NAME.to_string()) {
                    caps.push("relay".to_string());
                }
                if protos.iter().any(|p| p == KAD_PROTOCOL) {
                    caps.push("kad".to_string());
                    for a in &info.listen_addrs {
                        self.swarm.behaviour_mut().kad.add_address(&peer_id, a.clone());
                    }
                }
                let is_relay = caps.iter().any(|c| c == "relay");
                let addrs: Vec<String> = info.listen_addrs.iter().filter(|a| shareable(a)).map(|a| a.to_string()).collect();
                self.seen(Sighting { peer: peer_id, addrs, caps: Some(caps), agent: Some(info.agent_version), source: "identify" });
                // behind NAT: use relays we meet
                let private = self.status.lock().map(|s| s.nat == "private").unwrap_or(false);
                if is_relay
                    && private
                    && self.relay_listening.is_empty()
                    && let Some(a) = info.listen_addrs.iter().find(|a| shareable(a))
                {
                    self.add_relay(a.clone().with(Protocol::P2p(peer_id)));
                }
            }
            BehaviourEvent::Kad(kad::Event::OutboundQueryProgressed {
                result: kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders { providers, .. })),
                ..
            }) => {
                for p in providers {
                    if p != *self.swarm.local_peer_id() {
                        self.seen(Sighting { peer: p, addrs: vec![], caps: Some(vec!["relay".into(), "kad".into()]), agent: None, source: "kad" });
                    }
                }
            }
            BehaviourEvent::Autonat(autonat::Event::StatusChanged { new, .. }) => {
                let nat = match new {
                    autonat::NatStatus::Public(_) => "public",
                    autonat::NatStatus::Private => "private",
                    autonat::NatStatus::Unknown => "unknown",
                };
                self.update(|s| s.nat = nat.into());
                if nat == "private" {
                    let relays: Vec<PeerId> = self.relays.iter().copied().collect();
                    tracing::info!("network: behind NAT; {} relay(s) known", relays.len());
                }
            }
            BehaviourEvent::RelayClient(relay::client::Event::ReservationReqAccepted { relay_peer_id, .. }) => {
                tracing::info!("network: reservation on relay {relay_peer_id}");
                self.update(|s| {
                    let r = relay_peer_id.to_string();
                    if !s.reservations.contains(&r) {
                        s.reservations.push(r);
                    }
                });
            }
            BehaviourEvent::Dcutr(e) => {
                tracing::debug!("network: hole punch with {}: {}", e.remote_peer_id, if e.result.is_ok() { "direct" } else { "failed" });
            }
            _ => {}
        }
    }
}
