//! HTTPS hosting for organization sites (2.7). One port serves the pages, the JSON API,
//! template assets and unary gRPC (`muninn.org.v1.Org`, see `docs/org-hosting.md`) over
//! HTTP/1.1 and HTTP/2 with TLS. Nothing listens until [`Host::start`], and by default only
//! on 127.0.0.1. Which orgs are served is each org's `hosted` flag, plus any mirrors.
//!
//! Certificates are short-lived: a self-signed one made at each start (rcgen), or, when
//! `[hosting.acme]` is enabled with a domain and an email, one from Let's Encrypt (the
//! `shortlived` profile) through ACME HTTP-01, renewed when a third of its life is left.
//! The ACME account key and certificates are kept in the encrypted store, never in files.
//!
//! Members may sign requests with the key of their org key exchange:
//! `Authorization: Muninn exchange=ID, ts=MS, sig=HEX`, where `sig` is
//! HMAC-SHA256(HKDF-SHA256(key, "muninn/org/web/v1"), "METHOD\nPATH\nTS\nhex(SHA256(BODY))").
//! Signatures more than five minutes off, or seen before, are refused.
use super::serve::{self, Page};
use crate::crypto::Key32;
use crate::error::{Error, Result};
use crate::prefs::{AcmePrefs, HostingPrefs};
use crate::store::{Store, now_ms};
use bytes::Bytes;
use hmac::{Hmac, Mac};
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full, Limited, StreamBody};
use hyper::body::{Frame, Incoming};
use hyper::header::{self, HeaderMap, HeaderValue};
use hyper::{Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
use instant_acme::{
    Account, AccountCredentials, AuthorizationStatus, ChallengeType, HttpClient, Identifier, NewAccount, NewOrder, OrderStatus, RetryPolicy,
};
use prost::Message;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::watch;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

type Body = BoxBody<Bytes, Infallible>;

/// Largest request body accepted.
pub const MAX_BODY: usize = 8 << 20;
/// How far a signed request's time may be from ours.
pub const SKEW_MS: i64 = 5 * 60 * 1000;
/// The gRPC service name.
pub const GRPC_SERVICE: &str = "muninn.org.v1.Org";
const WEB_INFO: &[u8] = b"muninn/org/web/v1";
const ACCOUNT_SETTING: &str = "hosting.acme.account";
const CERT_SETTING: &str = "hosting.acme.cert";

fn tls_err(e: impl std::fmt::Display) -> Error {
    Error::Crypto(format!("tls: {e}"))
}

fn acme_err(e: impl std::fmt::Display) -> Error {
    Error::Other(format!("acme: {e}"))
}

// ---------------------------------------------------------------------------------------
// Member signatures

fn web_key(k: &Key32) -> Zeroizing<[u8; 32]> {
    let mut out = Zeroizing::new([0u8; 32]);
    hkdf::Hkdf::<Sha256>::new(None, &k.0).expand(WEB_INFO, out.as_mut()).expect("32 bytes is a valid HKDF length");
    out
}

fn canonical(method: &str, path: &str, ts: i64, body: &[u8]) -> Vec<u8> {
    format!("{}\n{}\n{}\n{}", method.to_ascii_uppercase(), path, ts, hex::encode(Sha256::digest(body))).into_bytes()
}

fn mac(k: &Key32) -> Hmac<Sha256> {
    <Hmac<Sha256> as Mac>::new_from_slice(web_key(k).as_ref()).expect("HMAC takes any key size")
}

/// The `Authorization` value for a request signed with an exchange key.
pub fn sign_with(key: &Key32, exchange: &str, method: &str, path: &str, body: &[u8], ts: i64) -> String {
    let mut m = mac(key);
    m.update(&canonical(method, path, ts, body));
    format!("Muninn exchange={exchange}, ts={ts}, sig={}", hex::encode(m.finalize().into_bytes()))
}

/// The `Authorization` value a member of `org` (an org joined here) sends with a request.
pub fn authorization(s: &Store, org: &str, method: &str, path: &str, body: &[u8]) -> Result<String> {
    let j = s.find_joined(org)?;
    let x = j.exchange_id.ok_or_else(|| Error::invalid("organization", "no key exchange with the org"))?;
    let key = s.exchange_key(&x)?.ok_or_else(|| Error::invalid("organization", "the key exchange has no key (unanswered or revoked)"))?;
    Ok(sign_with(&key, &x, method, path, body, now_ms()))
}

struct Auth {
    exchange: String,
    ts: i64,
    sig: Vec<u8>,
}

fn parse_auth(h: &str) -> Option<Auth> {
    let rest = h.trim().strip_prefix("Muninn ")?;
    let (mut exchange, mut ts, mut sig) = (None, None, None);
    for part in rest.split(',') {
        let (k, v) = part.trim().split_once('=')?;
        match k.trim() {
            "exchange" => exchange = Some(v.trim().to_string()),
            "ts" => ts = v.trim().parse().ok(),
            "sig" => sig = hex::decode(v.trim()).ok(),
            _ => {}
        }
    }
    Some(Auth { exchange: exchange?, ts: ts?, sig: sig? })
}

/// Signatures already seen (kept for twice the allowed skew).
#[derive(Default)]
pub struct Replay {
    seen: HashMap<Vec<u8>, i64>,
}

impl Replay {
    fn fresh(&mut self, sig: &[u8], now: i64) -> bool {
        self.seen.retain(|_, t| now - *t <= 2 * SKEW_MS);
        self.seen.insert(sig.to_vec(), now).is_none()
    }
}

/// Who signed a request: `Ok(None)` when it is not signed the Muninn way, the exchange id
/// when the signature is good, and why not otherwise.
pub fn verify(
    s: &Store,
    header: Option<&str>,
    method: &str,
    path: &str,
    body: &[u8],
    now: i64,
    replay: &mut Replay,
) -> std::result::Result<Option<String>, &'static str> {
    let Some(h) = header.filter(|h| h.trim_start().starts_with("Muninn ")) else { return Ok(None) };
    let a = parse_auth(h).ok_or("malformed Muninn authorization")?;
    if (now - a.ts).abs() > SKEW_MS {
        return Err("the signature is too old (or from the future)");
    }
    let key = s.exchange_key(&a.exchange).ok().flatten().ok_or("unknown key exchange")?;
    let mut m = mac(&key);
    m.update(&canonical(method, path, a.ts, body));
    m.verify_slice(&a.sig).map_err(|_| "bad signature")?;
    if !replay.fresh(&a.sig, now) {
        return Err("signature already used");
    }
    Ok(Some(a.exchange))
}

// ---------------------------------------------------------------------------------------
// Certificates

/// A certificate chain and its private key, as PEM. The key is wiped when dropped.
#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct CertPem {
    pub chain: String,
    pub key: String,
}

impl std::fmt::Debug for CertPem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CertPem").field("chain", &self.chain.len()).field("key", &"<redacted>").finish()
    }
}

/// What a certificate says (no secrets).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CertInfo {
    /// `self-signed` or `acme`.
    pub source: String,
    pub names: Vec<String>,
    pub not_before: i64,
    pub not_after: i64,
    pub issuer: String,
}

impl CertInfo {
    /// Renew when less than a third of the certificate's life is left.
    pub fn needs_renewal(&self, now: i64) -> bool {
        let life = (self.not_after - self.not_before).max(1);
        self.not_after - now < life / 3
    }
}

/// A self-signed certificate for these names, valid for `days`.
pub fn self_signed(names: &[String], days: u32) -> Result<CertPem> {
    let mut p = rcgen::CertificateParams::new(names.to_vec()).map_err(tls_err)?;
    let now = time::OffsetDateTime::now_utc();
    p.not_before = now - time::Duration::minutes(5);
    p.not_after = now + time::Duration::days(i64::from(days.max(1)));
    p.distinguished_name.push(rcgen::DnType::CommonName, names.first().map(String::as_str).unwrap_or("localhost"));
    p.distinguished_name.push(rcgen::DnType::OrganizationName, "Muninn (self-signed)");
    let key = rcgen::KeyPair::generate().map_err(tls_err)?;
    let cert = p.self_signed(&key).map_err(tls_err)?;
    Ok(CertPem { chain: cert.pem(), key: key.serialize_pem() })
}

/// Read a certificate chain's leaf.
pub fn cert_info(c: &CertPem, source: &str) -> Result<CertInfo> {
    let der = CertificateDer::pem_slice_iter(c.chain.as_bytes()).next().ok_or_else(|| tls_err("no certificate in the chain"))?.map_err(tls_err)?;
    let (_, x) = x509_parser::parse_x509_certificate(&der).map_err(tls_err)?;
    let mut names = vec![];
    if let Ok(Some(san)) = x.subject_alternative_name() {
        for n in &san.value.general_names {
            match n {
                x509_parser::extensions::GeneralName::DNSName(d) => names.push(d.to_string()),
                x509_parser::extensions::GeneralName::IPAddress(b) => match b.len() {
                    4 => names.push(IpAddr::from(<[u8; 4]>::try_from(*b).unwrap_or_default()).to_string()),
                    16 => names.push(IpAddr::from(<[u8; 16]>::try_from(*b).unwrap_or_default()).to_string()),
                    _ => {}
                },
                _ => {}
            }
        }
    }
    let v = x.validity();
    Ok(CertInfo {
        source: source.to_string(),
        names,
        not_before: v.not_before.timestamp() * 1000,
        not_after: v.not_after.timestamp() * 1000,
        issuer: x.issuer().to_string(),
    })
}

fn certified(c: &CertPem) -> Result<Arc<CertifiedKey>> {
    let chain = CertificateDer::pem_slice_iter(c.chain.as_bytes()).collect::<std::result::Result<Vec<_>, _>>().map_err(tls_err)?;
    if chain.is_empty() {
        return Err(tls_err("no certificate in the chain"));
    }
    let key = PrivateKeyDer::from_pem_slice(c.key.as_bytes()).map_err(tls_err)?;
    let signer = rustls::crypto::aws_lc_rs::sign::any_supported_type(&key).map_err(tls_err)?;
    Ok(Arc::new(CertifiedKey::new(chain, signer)))
}

/// Hands out the current certificate; swapped in place when it is renewed.
#[derive(Debug)]
struct Resolver(RwLock<Arc<CertifiedKey>>);

impl ResolvesServerCert for Resolver {
    fn resolve(&self, _: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        self.0.read().ok().map(|c| c.clone())
    }
}

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
struct StoredCert {
    domain: String,
    chain: String,
    key: String,
}

/// The ACME certificate kept for `domain`, if any.
pub fn stored_cert(s: &Store, domain: &str) -> Result<Option<CertPem>> {
    let Some(v) = s.setting(CERT_SETTING)?.map(Zeroizing::new) else { return Ok(None) };
    let c: StoredCert = serde_json::from_str(&v)?;
    Ok((c.domain == domain).then(|| CertPem { chain: c.chain.clone(), key: c.key.clone() }))
}

fn save_cert(s: &Store, domain: &str, c: &CertPem) -> Result<()> {
    let v = Zeroizing::new(serde_json::to_string(&StoredCert { domain: domain.into(), chain: c.chain.clone(), key: c.key.clone() })?);
    s.set_setting(CERT_SETTING, Some(&v))
}

// ---------------------------------------------------------------------------------------
// ACME

/// ACME settings (from `[hosting.acme]`).
#[derive(Debug, Clone, PartialEq)]
pub struct Acme {
    pub directory: String,
    pub domain: String,
    pub email: String,
    pub profile: String,
}

impl Acme {
    /// `None` unless ACME is enabled; an error when it is enabled without a domain or email.
    pub fn from_prefs(p: &AcmePrefs) -> Result<Option<Acme>> {
        if !p.enabled {
            return Ok(None);
        }
        if p.domain.trim().is_empty() || p.email.trim().is_empty() || !p.email.contains('@') {
            return Err(Error::invalid("hosting.acme", "needs a domain and an email (mnnn org cert acme --domain D --email E)"));
        }
        Ok(Some(Acme {
            directory: p.directory_url(),
            domain: p.domain.trim().to_string(),
            email: p.email.trim().to_string(),
            profile: p.profile.trim().to_string(),
        }))
    }
}

/// Makes the HTTP client ACME talks through (tests give a mock CA; `None`: the real one).
pub type AcmeHttp = Arc<dyn Fn() -> Box<dyn HttpClient> + Send + Sync>;

/// HTTP-01 tokens being answered: token → key authorization.
pub type Challenges = Arc<RwLock<HashMap<String, String>>>;

#[derive(Serialize, Deserialize)]
struct StoredAccount {
    directory: String,
    credentials: AccountCredentials,
}

fn with_store<T>(m: &Mutex<Store>, f: impl FnOnce(&Store) -> Result<T>) -> Result<T> {
    let s = m.lock().map_err(|_| Error::Other("store lock poisoned".into()))?;
    f(&s)
}

async fn account(store: &Mutex<Store>, acme: &Acme, http: Option<&AcmeHttp>) -> Result<Account> {
    let builder = || -> Result<instant_acme::AccountBuilder> {
        Ok(match http {
            Some(h) => Account::builder_with_http(h()),
            None => Account::builder().map_err(acme_err)?,
        })
    };
    let saved = with_store(store, |s| Ok(s.setting(ACCOUNT_SETTING)?.map(Zeroizing::new)))?;
    if let Some(v) = saved
        && let Ok(a) = serde_json::from_str::<StoredAccount>(&v)
        && a.directory == acme.directory
    {
        return builder()?.from_credentials(a.credentials).await.map_err(acme_err);
    }
    let contact = format!("mailto:{}", acme.email);
    let (account, credentials) = builder()?
        .create(&NewAccount { contact: &[&contact], terms_of_service_agreed: true, only_return_existing: false }, acme.directory.clone(), None)
        .await
        .map_err(acme_err)?;
    let v = Zeroizing::new(serde_json::to_string(&StoredAccount { directory: acme.directory.clone(), credentials })?);
    with_store(store, |s| s.set_setting(ACCOUNT_SETTING, Some(&v)))?;
    Ok(account)
}

/// Get a certificate for `acme.domain` through HTTP-01 (the tokens are answered from
/// `challenges`, served on the ACME HTTP port) and keep it in the store.
pub async fn obtain(store: &Mutex<Store>, acme: &Acme, http: Option<&AcmeHttp>, challenges: &Challenges) -> Result<CertPem> {
    let account = account(store, acme, http).await?;
    let ids = [Identifier::Dns(acme.domain.clone())];
    let mut order = match acme.profile.as_str() {
        "" => account.new_order(&NewOrder::new(&ids)).await,
        p => match account.new_order(&NewOrder::new(&ids).profile(p)).await {
            Ok(o) => Ok(o),
            // a CA without that profile: take its default
            Err(_) => account.new_order(&NewOrder::new(&ids)).await,
        },
    }
    .map_err(acme_err)?;
    let mut tokens = vec![];
    let ready = async {
        let mut auths = order.authorizations();
        while let Some(a) = auths.next().await {
            let mut a = a.map_err(acme_err)?;
            match a.status {
                AuthorizationStatus::Pending => {}
                AuthorizationStatus::Valid => continue,
                other => return Err(acme_err(format!("authorization is {other:?}"))),
            }
            let mut ch = a.challenge(ChallengeType::Http01).ok_or_else(|| acme_err("the CA offers no HTTP-01 challenge"))?;
            let token = ch.token.clone();
            challenges.write().map_err(|_| acme_err("challenge lock"))?.insert(token.clone(), ch.key_authorization().as_str().to_string());
            tokens.push(token);
            ch.set_ready().await.map_err(acme_err)?;
        }
        Ok(())
    }
    .await;
    let status = match ready {
        Ok(()) => order.poll_ready(&RetryPolicy::new().timeout(Duration::from_secs(120))).await.map_err(acme_err),
        Err(e) => Err(e),
    };
    if let Ok(mut c) = challenges.write() {
        for t in &tokens {
            c.remove(t);
        }
    }
    let status = status?;
    if status != OrderStatus::Ready {
        return Err(acme_err(format!("the order is {status:?}")));
    }
    let key = rcgen::KeyPair::generate().map_err(tls_err)?;
    let mut p = rcgen::CertificateParams::new(vec![acme.domain.clone()]).map_err(tls_err)?;
    p.distinguished_name = rcgen::DistinguishedName::new();
    let csr = p.serialize_request(&key).map_err(tls_err)?;
    order.finalize_csr(csr.der()).await.map_err(acme_err)?;
    let chain = order.poll_certificate(&RetryPolicy::new().timeout(Duration::from_secs(120))).await.map_err(acme_err)?;
    let c = CertPem { chain, key: key.serialize_pem() };
    cert_info(&c, "acme")?;
    with_store(store, |s| {
        save_cert(s, &acme.domain, &c)?;
        s.note_action(
            &crate::store::NewAction::new("org.host.cert", "host", format!("certificate for {} from ACME", acme.domain))
                .params(serde_json::json!({ "domain": acme.domain, "directory": acme.directory })),
        );
        Ok(())
    })?;
    Ok(c)
}

// ---------------------------------------------------------------------------------------
// gRPC messages (see docs/org-hosting.md for the .proto)

#[derive(Clone, PartialEq, prost::Message)]
pub struct OrgRequest {
    /// The org's slug or id.
    #[prost(string, tag = "1")]
    pub org: String,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct ArtifactRequest {
    #[prost(string, tag = "1")]
    pub org: String,
    #[prost(string, tag = "2")]
    pub id: String,
    /// `json` (default), `html`, `pdf`, `docx`, `latex`, `text`, `mnnp`.
    #[prost(string, tag = "3")]
    pub format: String,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct CallRequest {
    #[prost(string, tag = "1")]
    pub org: String,
    #[prost(string, tag = "2")]
    pub endpoint: String,
    /// The JSON input (`{}` when empty).
    #[prost(string, tag = "3")]
    pub input_json: String,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct JsonReply {
    #[prost(string, tag = "1")]
    pub json: String,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct ArtifactReply {
    #[prost(string, tag = "1")]
    pub mime: String,
    #[prost(bytes = "vec", tag = "2")]
    pub data: Vec<u8>,
}

/// One length-prefixed gRPC message.
pub fn grpc_frame(msg: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(msg.len() + 5);
    v.push(0);
    v.extend_from_slice(&(msg.len() as u32).to_be_bytes());
    v.extend_from_slice(msg);
    v
}

/// The message in a gRPC request body (uncompressed, exactly one).
pub fn grpc_unframe(b: &[u8]) -> std::result::Result<&[u8], (u32, String)> {
    if b.len() < 5 {
        return Err((3, "no gRPC message".into()));
    }
    if b[0] != 0 {
        return Err((12, "compressed messages are not supported".into()));
    }
    let n = u32::from_be_bytes([b[1], b[2], b[3], b[4]]) as usize;
    b.get(5..5 + n).filter(|_| b.len() == 5 + n).ok_or((3, "bad gRPC message length".into()))
}

fn safe_part(v: &str) -> bool {
    !v.is_empty() && v.len() <= 128 && v.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn grpc_code(status: u16) -> u32 {
    match status {
        400 | 413 => 3,
        401 => 16,
        403 => 7,
        404 => 5,
        405 => 12,
        _ => 13,
    }
}

fn page_error(p: &Page) -> (u32, String) {
    let msg = serde_json::from_slice::<serde_json::Value>(&p.body)
        .ok()
        .and_then(|v| v["error"].as_str().map(str::to_string))
        .unwrap_or_else(|| format!("status {}", p.status));
    (grpc_code(p.status), msg)
}

fn grpc_dispatch(s: &Store, path: &str, body: &[u8], who: Option<&str>) -> std::result::Result<Vec<u8>, (u32, String)> {
    if path.starts_with(&format!("/{}.", super::exec::PACKAGE)) {
        return grpc_exec(s, path, body, who);
    }
    let Some(method) = path.strip_prefix(&format!("/{GRPC_SERVICE}/")) else { return Err((12, format!("unknown service {path}"))) };
    let msg = grpc_unframe(body)?;
    let bad = |e: prost::DecodeError| (3, format!("bad message: {e}"));
    let check = |v: &str, what: &str| if safe_part(v) { Ok(()) } else { Err((3, format!("bad {what}"))) };
    let json = |p: Page| {
        if p.status == 200 { Ok(JsonReply { json: String::from_utf8_lossy(&p.body).into_owned() }.encode_to_vec()) } else { Err(page_error(&p)) }
    };
    match method {
        "GetOrg" | "ListArtifacts" | "ListEndpoints" => {
            let q = OrgRequest::decode(msg).map_err(bad)?;
            check(&q.org, "org")?;
            let what = match method {
                "GetOrg" => "org",
                "ListArtifacts" => "artifacts",
                _ => "endpoints",
            };
            json(serve::route(s, "GET", &format!("/o/{}/api/{what}", q.org), b"", who))
        }
        "GetArtifact" => {
            let q = ArtifactRequest::decode(msg).map_err(bad)?;
            check(&q.org, "org")?;
            check(&q.id, "artifact id")?;
            let format = if q.format.is_empty() { "json".to_string() } else { q.format };
            check(&format, "format")?;
            let p = serve::route(s, "GET", &format!("/o/{}/a/{}.{format}", q.org, q.id), b"", who);
            if p.status == 200 { Ok(ArtifactReply { mime: p.mime, data: p.body }.encode_to_vec()) } else { Err(page_error(&p)) }
        }
        "Call" => {
            let q = CallRequest::decode(msg).map_err(bad)?;
            check(&q.org, "org")?;
            check(&q.endpoint, "endpoint")?;
            let input = if q.input_json.trim().is_empty() { "{}".to_string() } else { q.input_json };
            json(serve::route(s, "POST", &format!("/o/{}/api/call/{}", q.org, q.endpoint), input.as_bytes(), who))
        }
        m => Err((12, format!("unknown method {m}"))),
    }
}

/// `muninn.code.v1.Executables/List` and each served space's `Run`.
fn grpc_exec(s: &Store, path: &str, body: &[u8], who: Option<&str>) -> std::result::Result<Vec<u8>, (u32, String)> {
    use super::exec::{self, pb};
    let msg = grpc_unframe(body)?;
    let bad = |e: prost::DecodeError| (3, format!("bad message: {e}"));
    let fail = |e: Error| {
        let m = e.to_string();
        (
            if m.contains("may not") {
                7
            } else if m.contains("is not served") {
                5
            } else {
                3
            },
            m,
        )
    };
    let (svc, method) = path.trim_start_matches('/').split_once('/').ok_or((12, format!("unknown method {path}")))?;
    let hidden = || (5, "no such org".to_string());
    if svc == exec::LIST_SERVICE {
        if method != "List" {
            return Err((12, format!("unknown method {method}")));
        }
        let q = pb::ListRequest::decode(msg).map_err(bad)?;
        let org = s.find_org(&q.org).map_err(|_| hidden())?;
        let a = serve::exec_caller(s, &org, who).ok_or_else(hidden)?;
        let list = exec::executables(s, &org, &a).map_err(fail)?;
        return Ok(pb::ExecutableList { executables: list.iter().map(|e| e.to_pb()).collect() }.encode_to_vec());
    }
    if method != "Run" {
        return Err((12, format!("unknown method {method}")));
    }
    let (pkg, short) = svc.rsplit_once('.').ok_or((12, format!("unknown service {svc}")))?;
    let org = s.orgs().map_err(fail)?.into_iter().find(|o| exec::package(o) == pkg).ok_or((12, format!("unknown service {svc}")))?;
    let a = serve::exec_caller(s, &org, who).ok_or_else(hidden)?;
    let input = exec::Input::decode(msg).map_err(bad)?;
    let v = exec::run(s, "grpc", &a, &org, short, &input).map_err(fail)?;
    Ok(exec::result_pb(&v).encode_to_vec())
}

fn percent(msg: &str) -> String {
    msg.bytes().map(|b| if (0x20..0x7f).contains(&b) && b != b'%' { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn grpc_response(r: std::result::Result<Vec<u8>, (u32, String)>) -> Response<Body> {
    let mut frames: Vec<std::result::Result<Frame<Bytes>, Infallible>> = vec![];
    let mut trailers = HeaderMap::new();
    match r {
        Ok(m) => {
            frames.push(Ok(Frame::data(Bytes::from(grpc_frame(&m)))));
            trailers.insert("grpc-status", HeaderValue::from_static("0"));
        }
        Err((code, msg)) => {
            trailers.insert("grpc-status", HeaderValue::from(code));
            if let Ok(v) = HeaderValue::from_str(&percent(&msg)) {
                trailers.insert("grpc-message", v);
            }
        }
    }
    frames.push(Ok(Frame::trailers(trailers)));
    Response::builder()
        .status(200)
        .header(header::CONTENT_TYPE, "application/grpc+proto")
        .body(StreamBody::new(futures::stream::iter(frames)).boxed())
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::new()).boxed()))
}

// ---------------------------------------------------------------------------------------
// The server

/// How to host (from `[hosting]`, with command-line overrides).
#[derive(Debug, Clone)]
pub struct HostOptions {
    pub bind: String,
    pub port: u16,
    pub names: Vec<String>,
    pub cert_days: u32,
    pub acme: Option<Acme>,
    pub acme_http_port: u16,
    /// Check the ACME certificate every ten minutes and renew it when due.
    pub auto_renew: bool,
}

impl HostOptions {
    pub fn from_prefs(p: &HostingPrefs) -> Result<HostOptions> {
        Ok(HostOptions {
            bind: p.bind.clone(),
            port: p.port,
            names: p.names.clone(),
            cert_days: p.cert_days,
            acme: Acme::from_prefs(&p.acme)?,
            acme_http_port: p.acme.http_port,
            auto_renew: true,
        })
    }

    /// Names the self-signed certificate covers.
    fn cert_names(&self) -> Vec<String> {
        let mut v = vec!["localhost".to_string(), "127.0.0.1".to_string()];
        if !matches!(self.bind.as_str(), "0.0.0.0" | "::" | "[::]") {
            v.push(self.bind.trim_matches(['[', ']']).to_string());
        }
        v.extend(self.names.iter().cloned());
        if let Some(a) = &self.acme {
            v.push(a.domain.clone());
        }
        let mut seen = std::collections::HashSet::new();
        v.retain(|n| !n.is_empty() && seen.insert(n.clone()));
        v
    }
}

struct State {
    store: Mutex<Store>,
    replay: Mutex<Replay>,
    challenges: Challenges,
    resolver: Arc<Resolver>,
    cert: RwLock<CertInfo>,
    acme: Option<Acme>,
    http: Option<AcmeHttp>,
}

impl State {
    fn install(&self, c: &CertPem, source: &str) -> Result<CertInfo> {
        let info = cert_info(c, source)?;
        *self.resolver.0.write().map_err(|_| tls_err("certificate lock"))? = certified(c)?;
        *self.cert.write().map_err(|_| tls_err("certificate lock"))? = info.clone();
        Ok(info)
    }

    fn cert(&self) -> CertInfo {
        self.cert.read().map(|c| c.clone()).unwrap_or_else(|e| e.into_inner().clone())
    }

    /// Renew through ACME when the certificate is not from ACME yet, or is due.
    async fn renew_if_due(&self, force: bool) -> Result<Option<CertInfo>> {
        let Some(acme) = &self.acme else { return Ok(None) };
        let cur = self.cert();
        if !force && cur.source == "acme" && !cur.needs_renewal(now_ms()) {
            return Ok(None);
        }
        // another process (`mnnn org cert renew`) may have renewed it
        if !force && let Some(c) = with_store(&self.store, |s| stored_cert(s, &acme.domain))? {
            let i = cert_info(&c, "acme")?;
            if !i.needs_renewal(now_ms()) {
                return self.install(&c, "acme").map(Some);
            }
        }
        let c = obtain(&self.store, acme, self.http.as_ref(), &self.challenges).await?;
        self.install(&c, "acme").map(Some)
    }
}

/// A running site server. Dropping it (or [`Host::stop`]) stops it.
pub struct Host {
    rt: Option<tokio::runtime::Runtime>,
    pub addr: SocketAddr,
    /// Where HTTP-01 challenges are answered (only with ACME).
    pub http_addr: Option<SocketAddr>,
    state: Arc<State>,
    stop: watch::Sender<bool>,
}

impl Host {
    pub fn start(store: Store, opts: HostOptions) -> Result<Host> {
        Host::start_with(store, opts, None)
    }

    /// Start with a given ACME HTTP client (tests use a mock CA).
    pub fn start_with(store: Store, opts: HostOptions, http: Option<AcmeHttp>) -> Result<Host> {
        let ip: IpAddr = opts
            .bind
            .trim_matches(['[', ']'])
            .parse()
            .map_err(|_| Error::invalid("hosting.bind", format!("`{}` is not an IP address", opts.bind)))?;
        let (pem, source) = match opts.acme.as_ref().map(|a| stored_cert(&store, &a.domain)).transpose()?.flatten() {
            Some(c) if cert_info(&c, "acme").is_ok_and(|i| i.not_after > now_ms()) => (c, "acme"),
            _ => (self_signed(&opts.cert_names(), opts.cert_days)?, "self-signed"),
        };
        let info = cert_info(&pem, source)?;
        let resolver = Arc::new(Resolver(RwLock::new(certified(&pem)?)));
        let mut tls = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(tls_err)?
            .with_no_client_auth()
            .with_cert_resolver(resolver.clone());
        tls.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls));
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).thread_name("mnnn-host").enable_all().build()?;
        let listener = rt.block_on(TcpListener::bind(SocketAddr::new(ip, opts.port)))?;
        let addr = listener.local_addr()?;
        let http_listener = match &opts.acme {
            Some(_) => Some(rt.block_on(TcpListener::bind(SocketAddr::new(ip, opts.acme_http_port)))?),
            None => None,
        };
        let http_addr = http_listener.as_ref().map(|l| l.local_addr()).transpose()?;
        let state = Arc::new(State {
            store: Mutex::new(store),
            replay: Mutex::new(Replay::default()),
            challenges: Arc::new(RwLock::new(HashMap::new())),
            resolver,
            cert: RwLock::new(info),
            acme: opts.acme.clone(),
            http,
        });
        let (stop, rx) = watch::channel(false);
        rt.spawn(accept_tls(listener, acceptor, state.clone(), rx.clone()));
        if let Some(l) = http_listener {
            rt.spawn(accept_http(l, state.challenges.clone(), rx.clone()));
        }
        if opts.acme.is_some() && opts.auto_renew {
            rt.spawn(renew_loop(state.clone(), rx));
        }
        Ok(Host { rt: Some(rt), addr, http_addr, state, stop })
    }

    /// The address to open (`https://127.0.0.1:8443/`).
    pub fn url(&self) -> String {
        let host = match self.addr.ip() {
            IpAddr::V4(v) if v.is_unspecified() => "127.0.0.1".to_string(),
            IpAddr::V6(v) if v.is_unspecified() => "[::1]".to_string(),
            IpAddr::V6(v) => format!("[{v}]"),
            IpAddr::V4(v) => v.to_string(),
        };
        format!("https://{host}:{}/", self.addr.port())
    }

    pub fn cert(&self) -> CertInfo {
        self.state.cert()
    }

    /// Get a fresh ACME certificate now (when ACME is set up).
    pub fn renew(&self) -> Result<Option<CertInfo>> {
        let rt = self.rt.as_ref().ok_or_else(|| Error::Other("the host is stopped".into()))?;
        rt.block_on(self.state.renew_if_due(true))
    }

    /// Use the host's store (it holds its own connection).
    pub fn with_store<T>(&self, f: impl FnOnce(&Store) -> Result<T>) -> Result<T> {
        with_store(&self.state.store, f)
    }

    /// What the host serves, for the status file.
    pub fn status(&self) -> HostFile {
        let (orgs, mirrors) = self
            .with_store(|s| {
                Ok((s.orgs()?.into_iter().filter(|o| o.hosted).map(|o| o.slug).collect(), s.mirrors()?.into_iter().map(|m| m.slug).collect()))
            })
            .unwrap_or_default();
        HostFile {
            pid: std::process::id(),
            url: self.url(),
            addr: self.addr.to_string(),
            http_addr: self.http_addr.map(|a| a.to_string()),
            cert: self.cert(),
            orgs,
            mirrors,
            updated_at: now_ms(),
        }
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        let _ = self.stop.send(true);
        if let Some(rt) = self.rt.take() {
            rt.shutdown_timeout(Duration::from_secs(2));
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.shutdown();
    }
}

async fn renew_loop(state: Arc<State>, mut stop: watch::Receiver<bool>) {
    loop {
        if let Err(e) = state.renew_if_due(false).await {
            tracing::warn!("certificate renewal failed: {e}");
        }
        tokio::select! {
            _ = stop.changed() => return,
            _ = tokio::time::sleep(Duration::from_secs(600)) => {}
        }
    }
}

async fn accept_tls(listener: TcpListener, acceptor: tokio_rustls::TlsAcceptor, state: Arc<State>, mut stop: watch::Receiver<bool>) {
    loop {
        tokio::select! {
            _ = stop.changed() => return,
            r = listener.accept() => {
                let Ok((tcp, _)) = r else { continue };
                let (acceptor, state) = (acceptor.clone(), state.clone());
                tokio::spawn(async move {
                    let Ok(Ok(tls)) = tokio::time::timeout(Duration::from_secs(10), acceptor.accept(tcp)).await else { return };
                    let svc = hyper::service::service_fn(move |req| handle(state.clone(), req));
                    let _ = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new()).serve_connection(TokioIo::new(tls), svc).await;
                });
            }
        }
    }
}

/// Plain HTTP, only for ACME HTTP-01 tokens.
async fn accept_http(listener: TcpListener, challenges: Challenges, mut stop: watch::Receiver<bool>) {
    loop {
        tokio::select! {
            _ = stop.changed() => return,
            r = listener.accept() => {
                let Ok((tcp, _)) = r else { continue };
                let challenges = challenges.clone();
                tokio::spawn(async move {
                    let svc = hyper::service::service_fn(move |req: Request<Incoming>| {
                        let token = req.uri().path().strip_prefix("/.well-known/acme-challenge/").map(str::to_string);
                        let answer = token.and_then(|t| challenges.read().ok().and_then(|c| c.get(&t).cloned()));
                        async move {
                            Ok::<_, Infallible>(match answer {
                                Some(a) => plain(200, "text/plain", a.into_bytes()),
                                None => plain(404, "text/plain", b"not found".to_vec()),
                            })
                        }
                    });
                    let _ = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(tcp), svc).await;
                });
            }
        }
    }
}

fn plain(status: u16, mime: &str, body: Vec<u8>) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        .body(Full::new(Bytes::from(body)).boxed())
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::new()).boxed()))
}

/// Security headers on every page. Scripts and styles may come from the site's own assets.
fn page_response(p: Page, hsts: bool) -> Response<Body> {
    let mut b = Response::builder()
        .status(p.status)
        .header(header::CONTENT_TYPE, p.mime.as_str())
        .header("x-content-type-options", "nosniff")
        .header("referrer-policy", "no-referrer")
        .header("content-security-policy", "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; frame-ancestors 'none'");
    if hsts {
        b = b.header("strict-transport-security", "max-age=86400");
    }
    if p.status == 401 {
        b = b.header(header::WWW_AUTHENTICATE, "Muninn");
    }
    b.body(Full::new(Bytes::from(p.body)).boxed()).unwrap_or_else(|_| Response::new(Full::new(Bytes::new()).boxed()))
}

async fn handle(state: Arc<State>, req: Request<Incoming>) -> std::result::Result<Response<Body>, Infallible> {
    let method = req.method().as_str().to_string();
    let path = req.uri().path_and_query().map(|p| p.as_str().to_string()).unwrap_or_else(|| "/".into());
    let grpc = req.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|c| c.starts_with("application/grpc"));
    let auth = req.headers().get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).map(str::to_string);
    let body = match Limited::new(req.into_body(), MAX_BODY).collect().await {
        Ok(b) => b.to_bytes(),
        Err(_) if grpc => return Ok(grpc_response(Err((8, "request too large".into())))),
        Err(_) => return Ok(page_response(Page::error(413, "request body too large"), false)),
    };
    let st = state.clone();
    let out = tokio::task::spawn_blocking(move || answer(&st, &method, &path, &body, auth.as_deref(), grpc)).await;
    Ok(out.unwrap_or_else(|_| page_response(Page::error(500, "internal error"), false)))
}

fn answer(st: &State, method: &str, path: &str, body: &[u8], auth: Option<&str>, grpc: bool) -> Response<Body> {
    let hsts = st.cert().source == "acme";
    let Ok(s) = st.store.lock() else { return page_response(Page::error(500, "store lock poisoned"), hsts) };
    let who = match st.replay.lock() {
        Ok(mut r) => verify(&s, auth, method, path, body, now_ms(), &mut r),
        Err(_) => Err("replay lock poisoned"),
    };
    let who = match who {
        Ok(w) => w,
        Err(e) if grpc => return grpc_response(Err((16, e.to_string()))),
        Err(e) => return page_response(Page::error(401, e), hsts),
    };
    if grpc {
        return grpc_response(grpc_dispatch(&s, path, body, who.as_deref()));
    }
    page_response(serve::route(&s, method, path, body, who.as_deref()), hsts)
}

// ---------------------------------------------------------------------------------------
// The running host as seen from other processes (`MUNINN_HOME/host/`; no secrets).

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostFile {
    pub pid: u32,
    pub url: String,
    pub addr: String,
    pub http_addr: Option<String>,
    pub cert: CertInfo,
    /// Slugs of the hosted orgs and of mirrors.
    pub orgs: Vec<String>,
    pub mirrors: Vec<String>,
    pub updated_at: i64,
}

pub fn dir(home: &Path) -> PathBuf {
    home.join("host")
}

fn host_file(home: &Path) -> PathBuf {
    dir(home).join("host.json")
}

fn stop_file(home: &Path) -> PathBuf {
    dir(home).join("stop")
}

pub fn write_status(home: &Path, f: &HostFile) -> Result<()> {
    std::fs::create_dir_all(dir(home))?;
    let tmp = dir(home).join("host.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(f)?)?;
    std::fs::rename(tmp, host_file(home))?;
    Ok(())
}

/// The host another process runs, if its file is fresh (updated in the last 15 s).
pub fn running(home: &Path) -> Option<HostFile> {
    let f: HostFile = serde_json::from_slice(&std::fs::read(host_file(home)).ok()?).ok()?;
    (now_ms() - f.updated_at < 15_000).then_some(f)
}

pub fn clear(home: &Path) {
    let _ = std::fs::remove_file(host_file(home));
    let _ = std::fs::remove_file(stop_file(home));
}

pub fn request_stop(home: &Path) -> Result<()> {
    std::fs::create_dir_all(dir(home))?;
    std::fs::write(stop_file(home), b"stop")?;
    Ok(())
}

/// Was a stop asked for? (Consumes the request.)
pub fn stop_requested(home: &Path) -> bool {
    let p = stop_file(home);
    p.exists() && std::fs::remove_file(p).is_ok()
}

#[cfg(test)]
mod tests;
