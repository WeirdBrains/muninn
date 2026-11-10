//! The stream transport (2.19): a gRPC bidirectional stream (`muninn.live.v1.Live/Open`)
//! between two nodes, over TLS 1.3 where each side proves its node identity with its libp2p
//! key (the libp2p TLS certificates), so a peer is the same peer id its key exchange names.
//! Calls are answered by the same [`super::wire::serve`] as the poll transport, and live
//! sessions are pushed (`subscribe`) by [`super::session::pull`], so consent, invites and
//! roles are checked identically on both. The node's listener is started by the running
//! node (`mnnn net start`, the TUI); a CLI command dials it.
//!
//! Which transport is used: `--transport`, else `MUNINN_COLLAB_TRANSPORT`, else the
//! `net.collab_transport` setting; `stream` (the default) or `poll`. A stream node falls back
//! to polling for a peer that runs `poll`, has no listener, or cannot be reached (NAT,
//! relay-only), records why (`net.collab.fallback`) and does not try again for a while.
use super::grpc::live_client::LiveClient;
use super::grpc::live_server::{Live, LiveServer};
use super::proto::{Batch, Envelope};
use super::wire::{CollabReq, CollabResp, Transport};
use crate::error::{Error, Result};
use crate::store::{Exchange, LiveSession, NewAction, Store, now_ms};
use libp2p::PeerId;
use libp2p::identity::Keypair;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::server::Connected;
use tonic::{Request, Response, Status, Streaming};

/// The authority of live-stream gRPC requests (the transport is libp2p TLS; this is only the URI).
pub(crate) const GRPC_AUTHORITY: &str = "http://muninn.live";

/// The stream listener's default TCP port (the node's is 4741).
pub const DEFAULT_PORT: u16 = 4742;
/// Overrides the setting (`stream` or `poll`).
pub const ENV: &str = "MUNINN_COLLAB_TRANSPORT";
/// Where the running node says its listener is (node-local settings).
pub const PORT_SETTING: &str = "collab.stream.port";
pub const ADVERTISE_SETTING: &str = "collab.stream.advertise";
/// A peer we poll for now: `collab.poll.PEER` → until (ms).
const FALLBACK_PREFIX: &str = "collab.poll.";
const FALLBACK_MS: i64 = 10 * 60_000;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const CALL_TIMEOUT: Duration = Duration::from_secs(120);
/// How often the host looks for changes to push.
const PUSH_EVERY: Duration = Duration::from_millis(150);

static OVERRIDE: OnceLock<Mode> = OnceLock::new();

/// How live collaboration reaches peers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Stream,
    Poll,
}

impl Mode {
    pub fn parse(v: &str) -> Result<Mode> {
        match v.trim().to_ascii_lowercase().as_str() {
            "" | "stream" | "grpc" => Ok(Mode::Stream),
            "poll" | "polling" => Ok(Mode::Poll),
            o => Err(Error::invalid("transport", format!("`{o}` (stream or poll)"))),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Stream => "stream",
            Mode::Poll => "poll",
        }
    }
}

/// `--transport` (once, at startup).
pub fn set_override(m: Mode) {
    let _ = OVERRIDE.set(m);
}

/// The transport to use: the flag, else the environment, else the setting.
pub fn mode(prefs: &crate::prefs::NetPrefs) -> Mode {
    if let Some(m) = OVERRIDE.get() {
        return *m;
    }
    if let Ok(v) = std::env::var(ENV)
        && let Ok(m) = Mode::parse(&v)
    {
        return m;
    }
    Mode::parse(&prefs.collab_transport).unwrap_or(Mode::Stream)
}

// ---- TLS with the node identity ----

/// A TLS connection and the peer it authenticated.
struct Tls<S> {
    io: S,
    peer: PeerId,
}

/// The authenticated peer of a stream (in each request's extensions).
#[derive(Debug, Clone, Copy)]
pub struct PeerInfo(pub PeerId);

impl Connected for Tls<tokio_rustls::server::TlsStream<TcpStream>> {
    type ConnectInfo = PeerInfo;
    fn connect_info(&self) -> PeerInfo {
        PeerInfo(self.peer)
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for Tls<S> {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>, buf: &mut ReadBuf<'_>) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut self.io).poll_read(cx, buf)
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Tls<S> {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>, b: &[u8]) -> std::task::Poll<std::io::Result<usize>> {
        Pin::new(&mut self.io).poll_write(cx, b)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut self.io).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut self.io).poll_shutdown(cx)
    }
}

fn peer_of(certs: Option<&[rustls::pki_types::CertificateDer<'_>]>) -> Option<PeerId> {
    let c = certs?.first()?;
    libp2p_tls::certificate::parse(c).ok().map(|c| c.peer_id())
}

fn err(e: impl std::fmt::Display) -> Error {
    Error::Other(e.to_string())
}

// ---- the server ----

struct Svc {
    store: Arc<Mutex<Store>>,
    me: String,
}

fn open_exchange(s: &Store, id: &str, peer: &str) -> std::result::Result<Exchange, String> {
    let x = s.exchange(id).map_err(|e| e.to_string())?.ok_or("unknown exchange")?;
    if !x.status.active() {
        return Err("this exchange is not active (revoked or never completed)".into());
    }
    if x.peer_id.as_deref() != Some(peer) {
        return Err("this exchange belongs to another node".into());
    }
    Ok(x)
}

impl Svc {
    fn answer(&self, peer: &str, e: &Envelope) -> Envelope {
        let mut out = Envelope { id: e.id, ..Default::default() };
        let r = (|| -> std::result::Result<Vec<u8>, String> {
            let mut s = self.store.lock().map_err(|_| "busy")?;
            let x = open_exchange(&s, &e.exchange, peer)?;
            let req: CollabReq = serde_json::from_slice(&e.call).map_err(|e| format!("request: {e}"))?;
            let resp = super::wire::serve(&mut s, &x, peer, &self.me, req).map_err(|e| e.to_string())?;
            serde_json::to_vec(&resp).map_err(|e| e.to_string())
        })();
        match r {
            Ok(b) => out.call = b,
            Err(m) => out.error = m,
        }
        out
    }
}

#[tonic::async_trait]
impl Live for Svc {
    type OpenStream = Pin<Box<dyn tokio_stream::Stream<Item = std::result::Result<Envelope, Status>> + Send>>;

    async fn open(&self, req: Request<Streaming<Envelope>>) -> std::result::Result<Response<Self::OpenStream>, Status> {
        let peer = req.extensions().get::<PeerInfo>().map(|p| p.0.to_string()).ok_or_else(|| Status::unauthenticated("no peer identity"))?;
        let mut inbound = req.into_inner();
        let (tx, rx) = tokio::sync::mpsc::channel::<std::result::Result<Envelope, Status>>(64);
        let svc = Arc::new(Svc { store: self.store.clone(), me: self.me.clone() });
        tokio::spawn(async move {
            while let Ok(Some(e)) = inbound.message().await {
                let (svc, tx, peer) = (svc.clone(), tx.clone(), peer.clone());
                if e.subscribe.is_empty() {
                    tokio::spawn(async move {
                        let out = tokio::task::spawn_blocking(move || svc.answer(&peer, &e)).await.unwrap_or_default();
                        let _ = tx.send(Ok(out)).await;
                    });
                } else {
                    tokio::spawn(push(svc, tx, peer, e));
                }
            }
        });
        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}

/// Follow a session for a subscriber: push what changed until it closes or they leave.
async fn push(svc: Arc<Svc>, tx: tokio::sync::mpsc::Sender<std::result::Result<Envelope, Status>>, peer: String, e: Envelope) {
    let mut since = e.since;
    let mut last: Option<Vec<(String, Option<i64>)>> = None;
    loop {
        let (svc2, peer2, sub, xid) = (svc.clone(), peer.clone(), e.subscribe.clone(), e.exchange.clone());
        let r = tokio::task::spawn_blocking(move || -> std::result::Result<(Batch, bool), String> {
            let s = svc2.store.lock().map_err(|_| "busy")?;
            let x = open_exchange(&s, &xid, &peer2)?;
            super::session::pull(&s, &peer2, &x.title(), &sub, since).map_err(|e| e.to_string())
        })
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
        match r {
            Err(m) => {
                let _ = tx.send(Ok(Envelope { id: e.id, error: m, ..Default::default() })).await;
                return;
            }
            Ok((b, closed)) => {
                let here: Vec<(String, Option<i64>)> = b.present.iter().map(|c| (c.label.clone(), c.offset)).collect();
                if !b.frames.is_empty() || last.as_ref() != Some(&here) {
                    since = b.head.max(since);
                    last = Some(here);
                    if tx.send(Ok(Envelope { id: e.id, push: Some(b), ..Default::default() })).await.is_err() {
                        return;
                    }
                }
                if closed {
                    return;
                }
            }
        }
        if tx.is_closed() {
            return;
        }
        tokio::time::sleep(PUSH_EVERY).await;
    }
}

/// The running stream listener.
pub struct Server {
    pub port: u16,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    /// Listen on `bind:port` (0: any port) and answer peers from `store`.
    pub fn start(store: Store, kp: &Keypair, bind: &str, port: u16) -> Result<Server> {
        let tls = Arc::new(libp2p_tls::make_server_config(kp).map_err(err)?);
        let me = kp.public().to_peer_id().to_string();
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().map_err(err)?;
        let addr = format!("{}:{port}", if bind.contains(':') && !bind.starts_with('[') { format!("[{bind}]") } else { bind.to_string() });
        let listener = rt.block_on(TcpListener::bind(&addr)).map_err(|e| Error::Other(format!("collab stream listener on {addr}: {e}")))?;
        let port = listener.local_addr().map_err(err)?.port();
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let svc = Svc { store: Arc::new(Mutex::new(store)), me };
        let thread = std::thread::spawn(move || {
            rt.block_on(async move {
                let (itx, irx) = tokio::sync::mpsc::channel(16);
                let acceptor = tokio_rustls::TlsAcceptor::from(tls);
                tokio::spawn(async move {
                    while let Ok((tcp, _)) = listener.accept().await {
                        let (acceptor, itx) = (acceptor.clone(), itx.clone());
                        tokio::spawn(async move {
                            let Ok(Ok(io)) = tokio::time::timeout(Duration::from_secs(10), acceptor.accept(tcp)).await else { return };
                            let Some(peer) = peer_of(io.get_ref().1.peer_certificates()) else { return };
                            let _ = itx.send(Ok::<_, std::io::Error>(Tls { io, peer })).await;
                        });
                    }
                });
                let _ = tonic::transport::Server::builder()
                    .add_service(LiveServer::new(svc))
                    .serve_with_incoming_shutdown(ReceiverStream::new(irx), async {
                        let _ = stopped.await;
                    })
                    .await;
            });
        });
        Ok(Server { port, stop: Some(stop), thread: Some(thread) })
    }

    pub fn halt(mut self) {
        if let Some(s) = self.stop.take() {
            let _ = s.send(());
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Start the listener for the running node when the mode is `stream`, and say where it is.
pub fn serve(store: &Store, listener_store: Store, kp: &Keypair, prefs: &crate::prefs::NetPrefs, origin: &str) -> Option<Server> {
    let _ = store.set_setting(PORT_SETTING, None);
    let _ = store.set_setting(ADVERTISE_SETTING, None);
    if mode(prefs) != Mode::Stream {
        return None;
    }
    match Server::start(listener_store, kp, &prefs.collab_bind, prefs.collab_port) {
        Ok(sv) => {
            let _ = store.set_setting(PORT_SETTING, Some(&sv.port.to_string()));
            if !prefs.collab_advertise.is_empty() {
                let _ = store.set_setting(ADVERTISE_SETTING, Some(&serde_json::to_string(&prefs.collab_advertise).unwrap_or_default()));
            }
            Some(sv)
        }
        Err(e) => {
            store.note_action(
                &NewAction::new("net.collab.fallback", origin, format!("no stream listener ({e}); peers will poll"))
                    .params(serde_json::json!({ "error": e.to_string() })),
            );
            None
        }
    }
}

/// Clear what [`serve`] said (the node stopped).
pub fn stopped(store: &Store) {
    let _ = store.set_setting(PORT_SETTING, None);
    let _ = store.set_setting(ADVERTISE_SETTING, None);
}

/// The answer to `Hello`: this node's transport and where its listener is.
pub fn hello(s: &Store) -> CollabResp {
    let port: u16 = s.setting(PORT_SETTING).ok().flatten().and_then(|p| p.parse().ok()).unwrap_or(0);
    let addrs: Vec<String> = s.setting(ADVERTISE_SETTING).ok().flatten().and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default();
    CollabResp::Hello { mode: if port > 0 { "stream".into() } else { "poll".into() }, port, addrs }
}

// ---- the client ----

/// A connection to one peer's stream listener.
pub struct StreamTransport {
    rt: tokio::runtime::Runtime,
    tx: tokio::sync::mpsc::Sender<Envelope>,
    rx: Mutex<Streaming<Envelope>>,
    exchange: String,
    next: AtomicU64,
}

/// Where to dial a peer's listener: what it advertises, else its node addresses with `port`.
pub fn candidates(node_addrs: &[libp2p::Multiaddr], port: u16, advertised: &[String]) -> Vec<String> {
    if !advertised.is_empty() {
        return advertised.to_vec();
    }
    use libp2p::multiaddr::Protocol;
    let mut v = vec![];
    for a in node_addrs {
        if a.iter().any(|p| matches!(p, Protocol::P2pCircuit)) {
            continue;
        }
        let host = a.iter().find_map(|p| match p {
            Protocol::Ip4(i) => Some(i.to_string()),
            Protocol::Ip6(i) => Some(format!("[{i}]")),
            Protocol::Dns(d) | Protocol::Dns4(d) | Protocol::Dns6(d) => Some(d.to_string()),
            _ => None,
        });
        if let Some(h) = host {
            let c = format!("{h}:{port}");
            if !v.contains(&c) {
                v.push(c);
            }
        }
    }
    v
}

impl StreamTransport {
    /// Dial `peer` at the first of `addrs` that answers, proving who both sides are.
    pub fn connect(kp: &Keypair, peer: PeerId, addrs: &[String], exchange: &str) -> Result<StreamTransport> {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(err)?;
        let cfg = Arc::new(libp2p_tls::make_client_config(kp, Some(peer)).map_err(err)?);
        let mut last = Error::Other("no address to dial".into());
        for a in addrs {
            let (cfg, a2) = (cfg.clone(), a.clone());
            let r = rt.block_on(async move {
                let ep = tonic::transport::Endpoint::from_static(GRPC_AUTHORITY).connect_timeout(CONNECT_TIMEOUT);
                let ch = tokio::time::timeout(
                    CONNECT_TIMEOUT * 2,
                    ep.connect_with_connector(tower::service_fn(move |_| {
                        let (cfg, a) = (cfg.clone(), a2.clone());
                        async move {
                            let tcp = TcpStream::connect(&a).await?;
                            let name = rustls::pki_types::ServerName::try_from("l").expect("name");
                            let io = tokio_rustls::TlsConnector::from(cfg).connect(name, tcp).await?;
                            Ok::<_, std::io::Error>(hyper_util::rt::TokioIo::new(Tls { io, peer }))
                        }
                    })),
                )
                .await
                .map_err(|_| Error::Other("timed out".into()))?
                .map_err(err)?;
                let (tx, rx) = tokio::sync::mpsc::channel::<Envelope>(64);
                let resp = LiveClient::new(ch).open(ReceiverStream::new(rx)).await.map_err(err)?;
                Ok::<_, Error>((tx, resp.into_inner()))
            });
            match r {
                Ok((tx, rx)) => return Ok(StreamTransport { rt, tx, rx: Mutex::new(rx), exchange: exchange.into(), next: AtomicU64::new(1) }),
                Err(e) => last = Error::Other(format!("{a}: {e}")),
            }
        }
        Err(last)
    }

    fn send(&self, e: Envelope) -> Result<()> {
        self.rt.block_on(self.tx.send(e)).map_err(|_| Error::Other("the stream closed".into()))
    }

    /// The next message for call `id` (within `CALL_TIMEOUT`, or whenever when following).
    fn recv(&self, id: u64, rx: &mut Streaming<Envelope>, follow: bool) -> Result<Envelope> {
        loop {
            let m = if follow {
                self.rt.block_on(rx.message()).map_err(err)?
            } else {
                self.rt
                    .block_on(async { tokio::time::timeout(CALL_TIMEOUT, rx.message()).await })
                    .map_err(|_| Error::Other("the peer did not answer in time".into()))?
                    .map_err(err)?
            };
            let m = m.ok_or_else(|| Error::Other("the stream closed".into()))?;
            if m.id == id {
                if !m.error.is_empty() {
                    return Err(Error::Other(m.error));
                }
                return Ok(m);
            }
        }
    }
}

impl Transport for StreamTransport {
    fn call(&self, req: CollabReq) -> Result<CollabResp> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let mut rx = self.rx.lock().map_err(|_| Error::Other("busy".into()))?;
        self.send(Envelope { id, exchange: self.exchange.clone(), call: serde_json::to_vec(&req)?, ..Default::default() })?;
        let m = self.recv(id, &mut rx, false)?;
        Ok(serde_json::from_slice(&m.call)?)
    }

    fn name(&self) -> &'static str {
        "stream"
    }

    fn follow(&self, s: &Store, ls: &LiveSession, each: &mut dyn FnMut(&super::session::State, &[super::proto::Cursor]) -> bool) -> Result<()> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let mut rx = self.rx.lock().map_err(|_| Error::Other("busy".into()))?;
        let since = s.live_head(&ls.id)? as u64;
        self.send(Envelope { id, exchange: self.exchange.clone(), subscribe: ls.id.clone(), since, ..Default::default() })?;
        loop {
            let m = self.recv(id, &mut rx, true)?;
            let Some(b) = m.push else { continue };
            let st = super::session::take(s, ls, &b, false)?;
            if !each(&st, &b.present) || st.closed {
                return Ok(());
            }
        }
    }
}

/// The transport for the peer behind `x`: a stream when this node and the peer both use
/// streams and the peer's listener answers; otherwise polling (and why is recorded once).
pub fn pick<'a>(s: &'a Store, node: &'a crate::net::node::Node, x: &'a Exchange, relays: &'a [String], m: Mode) -> Box<dyn Transport + 'a> {
    let poll = Box::new(super::wire::Libp2p { store: s, node, exchange: x, relays });
    if m == Mode::Poll {
        return poll;
    }
    let Some(pid) = x.peer_id.clone() else { return poll };
    let key = format!("{FALLBACK_PREFIX}{pid}");
    if s.setting(&key).ok().flatten().and_then(|v| v.parse::<i64>().ok()).is_some_and(|until| until > now_ms()) {
        return poll;
    }
    let why = match try_stream(s, node, x, relays, &pid) {
        Ok(t) => {
            let _ = s.set_setting(&key, None);
            return Box::new(t);
        }
        Err(why) => why,
    };
    let _ = s.set_setting(&key, Some(&(now_ms() + FALLBACK_MS).to_string()));
    s.note_action(&NewAction::new("net.collab.fallback", "network", format!("polling {}: {why}", x.title())).params(serde_json::json!({
        "peer": pid, "exchange": x.id, "reason": why,
    })));
    poll
}

fn try_stream(s: &Store, node: &crate::net::node::Node, x: &Exchange, relays: &[String], pid: &str) -> std::result::Result<StreamTransport, String> {
    let poll = super::wire::Libp2p { store: s, node, exchange: x, relays };
    let (mode, port, addrs) = match poll.call(CollabReq::Hello) {
        Ok(CollabResp::Hello { mode, port, addrs }) => (mode, port, addrs),
        Ok(_) => return Err("unexpected answer to hello".into()),
        Err(e) => return Err(format!("no hello ({e})")),
    };
    if mode != "stream" || port == 0 {
        return Err("the peer uses polling".into());
    }
    let peer: PeerId = pid.parse().map_err(|_| "bad peer id".to_string())?;
    let kp = crate::net::identity::load_or_create(s).map_err(|e| e.to_string())?;
    let dial = candidates(&crate::net::client::peer_addrs(s, pid, relays), port, &addrs);
    StreamTransport::connect(&kp, peer, &dial, &x.id).map_err(|e| format!("its stream listener did not answer ({e})"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_and_dial_candidates() {
        assert_eq!(Mode::parse("Poll").unwrap(), Mode::Poll);
        assert_eq!(Mode::parse("").unwrap(), Mode::Stream);
        assert!(Mode::parse("carrier-pigeon").is_err());
        let a: Vec<libp2p::Multiaddr> =
            ["/ip4/10.0.0.2/tcp/4741", "/ip4/10.0.0.2/udp/4741/quic-v1", "/ip6/::1/tcp/4741", "/dns4/alien.example/tcp/4741"]
                .iter()
                .map(|a| a.parse().unwrap())
                .collect();
        assert_eq!(candidates(&a, 4742, &[]), ["10.0.0.2:4742", "[::1]:4742", "alien.example:4742"]);
        assert_eq!(candidates(&a, 4742, &["pub.example:9000".into()]), ["pub.example:9000"]);
    }
}
