use super::*;
use crate::org::{Actor, create, edit, set_published, transfer};
use crate::store::{ExchangeStatus, MemberStatus, NewExchange, OrgChange, Published, ShareKind, Visibility};
use base64::Engine;
use http_body_util::Full;
use instant_acme::{BodyWrapper, BytesResponse};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use std::pin::Pin;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const KEY: [u8; 32] = [7u8; 32];

fn keyed_exchange(s: &Store, id: &str) {
    let n = NewExchange { id: id.into(), label: "ben".into(), role: "answerer".into(), status: Some(ExchangeStatus::Incoming), ..Default::default() };
    s.insert_exchange(&n, None).unwrap();
    s.complete_exchange(id, "12D3KooWbenben", "ben", "answer", &Key32(KEY), "12345-67890", "fp").unwrap();
}

/// A hosted public org with one public and one members-only journal, and a member (ben)
/// whose exchange key signs web requests.
fn hosted_lab(s: &Store) -> (String, String) {
    let o = create(s, "cli", "Lab", None, "we look").unwrap();
    let o = edit(
        s,
        "cli",
        &Actor::Owner,
        &o,
        &OrgChange { hosted: Some(true), member_auth: Some(true), visibility: Some(Visibility::Public), ..Default::default() },
    )
    .unwrap();
    for (name, text) in [("notes", "a bright comet"), ("inner", "members only")] {
        let j = s.create_journal(name, None, crate::store::EntryFormat::Markdown).unwrap();
        s.add_entry(&j.id, "Entry", text, None).unwrap();
    }
    let (pubr, _) = transfer(s, "cli", &Actor::Owner, &o, ShareKind::Journal, "notes", None).unwrap();
    set_published(s, "cli", &Actor::Owner, &o, &pubr, Published::Public).unwrap();
    let (memr, _) = transfer(s, "cli", &Actor::Owner, &o, ShareKind::Journal, "inner", None).unwrap();
    set_published(s, "cli", &Actor::Owner, &o, &memr, Published::Members).unwrap();
    let viewer = s.find_role(&o.id, "viewer").unwrap();
    s.upsert_member(&o.id, "12D3KooWbenben", "ben", Some(&viewer.id), MemberStatus::Member, Some("x-ben"), "", None).unwrap();
    keyed_exchange(s, "x-ben");
    (pubr.id, memr.id)
}

fn opts() -> HostOptions {
    HostOptions { bind: "127.0.0.1".into(), port: 0, names: vec![], cert_days: 2, acme: None, acme_http_port: 0, auto_renew: false }
}

#[test]
fn signatures_verify_once_within_five_minutes() {
    let s = Store::open_in_memory().unwrap();
    keyed_exchange(&s, "x1");
    let now = now_ms();
    let h = sign_with(&Key32(KEY), "x1", "post", "/o/lab/api/call/q", b"{}", now);
    assert!(!h.contains(&hex::encode(KEY)), "the key never travels");
    let mut r = Replay::default();
    assert_eq!(verify(&s, Some(&h), "POST", "/o/lab/api/call/q", b"{}", now, &mut r), Ok(Some("x1".into())));
    assert!(verify(&s, Some(&h), "POST", "/o/lab/api/call/q", b"{}", now, &mut r).is_err(), "no replays");
    let h = sign_with(&Key32(KEY), "x1", "POST", "/o/lab/api/call/q", b"{}", now);
    assert!(verify(&s, Some(&h), "POST", "/o/lab/api/call/q", b"{\"x\":1}", now, &mut Replay::default()).is_err(), "the body is signed");
    assert!(verify(&s, Some(&h), "POST", "/o/lab/api/call/other", b"{}", now, &mut Replay::default()).is_err(), "the path is signed");
    assert!(verify(&s, Some(&h), "POST", "/o/lab/api/call/q", b"{}", now + SKEW_MS + 1, &mut Replay::default()).is_err(), "too old");
    let wrong = sign_with(&Key32([8u8; 32]), "x1", "POST", "/", b"", now);
    assert_eq!(verify(&s, Some(&wrong), "POST", "/", b"", now, &mut Replay::default()), Err("bad signature"));
    let ghost = sign_with(&Key32(KEY), "nope", "GET", "/", b"", now);
    assert!(verify(&s, Some(&ghost), "GET", "/", b"", now, &mut Replay::default()).is_err());
    assert_eq!(verify(&s, Some("Bearer abc"), "GET", "/", b"", now, &mut Replay::default()), Ok(None), "other schemes are not ours");
    assert_eq!(verify(&s, None, "GET", "/", b"", now, &mut Replay::default()), Ok(None));
}

#[test]
fn self_signed_certificates_are_short_and_name_this_machine() {
    let o = HostOptions { names: vec!["lab.local".into()], ..opts() };
    let c = self_signed(&o.cert_names(), 3).unwrap();
    let i = cert_info(&c, "self-signed").unwrap();
    assert!(i.names.contains(&"localhost".to_string()) && i.names.contains(&"127.0.0.1".to_string()) && i.names.contains(&"lab.local".to_string()));
    let days = (i.not_after - now_ms()) as f64 / 86_400_000.0;
    assert!((2.9..3.1).contains(&days), "{days}");
    assert!(!i.needs_renewal(now_ms()));
    assert!(i.needs_renewal(i.not_after - (i.not_after - i.not_before) / 4));
    assert!(!format!("{c:?}").contains("PRIVATE"), "debug output hides the key");
}

// --- a TLS client that accepts any certificate (and remembers it)

#[derive(Debug)]
struct AnyCert(Arc<Mutex<Option<Vec<u8>>>>);

fn provider() -> rustls::crypto::CryptoProvider {
    rustls::crypto::aws_lc_rs::default_provider()
}

impl ServerCertVerifier for AnyCert {
    fn verify_server_cert(
        &self,
        ee: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        *self.0.lock().unwrap() = Some(ee.to_vec());
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        d: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(m, c, d, &provider().signature_verification_algorithms)
    }
    fn verify_tls13_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        d: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(m, c, d, &provider().signature_verification_algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        provider().signature_verification_algorithms.supported_schemes()
    }
}

struct Reply {
    status: u16,
    headers: HeaderMap,
    body: Vec<u8>,
    trailers: Option<HeaderMap>,
    cert: Vec<u8>,
}

fn send(addr: SocketAddr, h2: bool, method: &str, path: &str, headers: &[(&str, String)], body: Vec<u8>) -> Reply {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(async move {
        let seen = Arc::new(Mutex::new(None));
        let mut cfg = rustls::ClientConfig::builder_with_provider(Arc::new(provider()))
            .with_safe_default_protocol_versions()
            .unwrap()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AnyCert(seen.clone())))
            .with_no_client_auth();
        cfg.alpn_protocols = vec![if h2 { b"h2".to_vec() } else { b"http/1.1".to_vec() }];
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let tls = tokio_rustls::TlsConnector::from(Arc::new(cfg)).connect(ServerName::try_from("localhost").unwrap(), tcp).await.unwrap();
        let mut req = Request::builder().method(method).uri(format!("https://localhost:{}{path}", addr.port()));
        for (k, v) in headers {
            req = req.header(*k, v.as_str());
        }
        if !h2 {
            req = req.header(header::HOST, "localhost");
        }
        let req = req.body(Full::new(Bytes::from(body))).unwrap();
        let resp = if h2 {
            let (mut tx, conn) = hyper::client::conn::http2::handshake(TokioExecutor::new(), TokioIo::new(tls)).await.unwrap();
            tokio::spawn(conn);
            tx.send_request(req).await.unwrap()
        } else {
            let (mut tx, conn) = hyper::client::conn::http1::handshake(TokioIo::new(tls)).await.unwrap();
            tokio::spawn(conn);
            tx.send_request(req).await.unwrap()
        };
        let status = resp.status().as_u16();
        let headers = resp.headers().clone();
        let all = resp.into_body().collect().await.unwrap();
        let trailers = all.trailers().cloned();
        let cert = seen.lock().unwrap().clone().unwrap_or_default();
        Reply { status, headers, body: all.to_bytes().to_vec(), trailers, cert }
    })
}

#[test]
fn the_host_serves_pages_json_and_signed_member_requests_over_https() {
    let s = Store::open_in_memory().unwrap();
    let (public, members) = hosted_lab(&s);
    let host = Host::start(s, opts()).unwrap();
    assert!(host.addr.ip().is_loopback(), "localhost by default");
    assert!(host.url().starts_with("https://127.0.0.1:"));
    let get = |h2: bool, p: &str, hs: &[(&str, String)]| send(host.addr, h2, "GET", p, hs, vec![]);
    let home = get(true, "/", &[]);
    assert_eq!(home.status, 200);
    assert!(String::from_utf8_lossy(&home.body).contains("/o/lab/"));
    assert_eq!(home.headers["x-content-type-options"], "nosniff");
    assert!(home.headers.contains_key("content-security-policy"));
    // HTTP/1.1 works too, and the certificate is ours (self-signed)
    let page = get(false, &format!("/o/lab/a/{public}"), &[]);
    assert!(page.status == 200 && String::from_utf8_lossy(&page.body).contains("bright comet"));
    let (_, x) = x509_parser::parse_x509_certificate(&page.cert).unwrap();
    assert!(x.issuer().to_string().contains("Muninn"));
    assert_eq!(host.cert().source, "self-signed");
    let api: serde_json::Value = serde_json::from_slice(&get(true, "/o/lab/api/org", &[]).body).unwrap();
    assert_eq!(api["slug"], "lab", "{api}");
    // members-only: hidden from guests, shown to a signed member, refused with a bad signature
    let p = format!("/o/lab/a/{members}.json");
    assert_eq!(get(true, &p, &[]).status, 404);
    let auth = sign_with(&Key32(KEY), "x-ben", "GET", &p, b"", now_ms());
    let ok = get(true, &p, &[("authorization", auth.clone())]);
    assert_eq!(ok.status, 200);
    assert!(String::from_utf8_lossy(&ok.body).contains("members only"));
    let again = get(true, &p, &[("authorization", auth)]);
    assert_eq!(again.status, 401, "a signature is good once");
    assert_eq!(again.headers[header::WWW_AUTHENTICATE], "Muninn");
    let forged = sign_with(&Key32([1u8; 32]), "x-ben", "GET", &p, b"", now_ms());
    assert_eq!(get(true, &p, &[("authorization", forged)]).status, 401);
    // members' downloads are in history
    assert!(host.with_store(|s| Ok(s.actions()?.iter().any(|a| a.kind == "org.serve"))).unwrap());
    let st = host.status();
    assert_eq!(st.orgs, vec!["lab".to_string()]);
    host.stop();
}

fn grpc(addr: SocketAddr, method: &str, msg: Vec<u8>, auth: Option<String>) -> Reply {
    let path = format!("/{GRPC_SERVICE}/{method}");
    let mut hs = vec![("content-type", "application/grpc".to_string()), ("te", "trailers".to_string())];
    if let Some(a) = auth {
        hs.push(("authorization", a));
    }
    send(addr, true, "POST", &path, &hs, grpc_frame(&msg))
}

fn grpc_status(r: &Reply) -> String {
    r.trailers.as_ref().and_then(|t| t.get("grpc-status")).map(|v| v.to_str().unwrap().to_string()).unwrap_or_default()
}

#[test]
fn the_host_answers_grpc() {
    let s = Store::open_in_memory().unwrap();
    let (public, members) = hosted_lab(&s);
    let host = Host::start(s, opts()).unwrap();
    let r = grpc(host.addr, "GetOrg", OrgRequest { org: "lab".into() }.encode_to_vec(), None);
    assert_eq!((r.status, grpc_status(&r).as_str()), (200, "0"));
    assert_eq!(r.headers[header::CONTENT_TYPE], "application/grpc+proto");
    let reply = JsonReply::decode(grpc_unframe(&r.body).unwrap()).unwrap();
    assert!(reply.json.contains("\"slug\""), "{}", reply.json);
    let r = grpc(host.addr, "ListArtifacts", OrgRequest { org: "lab".into() }.encode_to_vec(), None);
    assert!(JsonReply::decode(grpc_unframe(&r.body).unwrap()).unwrap().json.contains("notes"));
    let r = grpc(host.addr, "GetArtifact", ArtifactRequest { org: "lab".into(), id: public, format: "html".into() }.encode_to_vec(), None);
    let a = ArtifactReply::decode(grpc_unframe(&r.body).unwrap()).unwrap();
    assert!(a.mime.starts_with("text/html") && String::from_utf8_lossy(&a.data).contains("bright comet"));
    // members-only over gRPC: NOT_FOUND for guests, fine when signed (the body is the frame)
    let msg = ArtifactRequest { org: "lab".into(), id: members, format: String::new() }.encode_to_vec();
    assert_eq!(grpc_status(&grpc(host.addr, "GetArtifact", msg.clone(), None)), "5");
    let auth = sign_with(&Key32(KEY), "x-ben", "POST", &format!("/{GRPC_SERVICE}/GetArtifact"), &grpc_frame(&msg), now_ms());
    let r = grpc(host.addr, "GetArtifact", msg, Some(auth));
    assert_eq!(grpc_status(&r), "0");
    assert!(String::from_utf8_lossy(&ArtifactReply::decode(grpc_unframe(&r.body).unwrap()).unwrap().data).contains("members only"));
    assert_eq!(grpc_status(&grpc(host.addr, "GetOrg", OrgRequest { org: "../etc".into() }.encode_to_vec(), None)), "3");
    assert_eq!(grpc_status(&grpc(host.addr, "Nope", vec![], None)), "12");
    assert_eq!(
        grpc_status(&grpc(
            host.addr,
            "Call",
            CallRequest { org: "lab".into(), endpoint: "none".into(), input_json: String::new() }.encode_to_vec(),
            None
        )),
        "5"
    );
    host.stop();
}

// --- a mock ACME CA (no network): it checks the HTTP-01 token on the host's challenge port

struct Ca {
    http_port: u16,
    token: String,
    valid: bool,
    issued: Option<String>,
    profile: Option<String>,
    accounts: u32,
    issuer: rcgen::Issuer<'static, rcgen::KeyPair>,
    ca_pem: String,
}

#[derive(Clone)]
struct MockCa(Arc<Mutex<Ca>>);

const BASE: &str = "https://ca.test";

impl MockCa {
    fn new() -> MockCa {
        let mut p = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
        p.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        p.distinguished_name.push(rcgen::DnType::CommonName, "Mock CA");
        let key = rcgen::KeyPair::generate().unwrap();
        let ca = p.self_signed(&key).unwrap();
        MockCa(Arc::new(Mutex::new(Ca {
            http_port: 0,
            token: "tok-123".into(),
            valid: false,
            issued: None,
            profile: None,
            accounts: 0,
            issuer: rcgen::Issuer::new(p, key),
            ca_pem: ca.pem(),
        })))
    }

    fn order(c: &Ca) -> serde_json::Value {
        let status = if c.issued.is_some() {
            "valid"
        } else if c.valid {
            "ready"
        } else {
            "pending"
        };
        let mut v = serde_json::json!({
            "status": status,
            "identifiers": [{"type": "dns", "value": "org.example.test"}],
            "authorizations": [format!("{BASE}/authz/1")],
            "finalize": format!("{BASE}/finalize/1"),
        });
        if c.issued.is_some() {
            v["certificate"] = format!("{BASE}/cert/1").into();
        }
        v
    }

    async fn answer(&self, method: String, url: String, body: Vec<u8>) -> (u16, Vec<(&'static str, String)>, Vec<u8>) {
        let json = |v: serde_json::Value| serde_json::to_vec(&v).unwrap();
        let payload: serde_json::Value = serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .and_then(|j| j["payload"].as_str().map(str::to_string))
            .filter(|p| !p.is_empty())
            .and_then(|p| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(serde_json::Value::Null);
        let path = url.strip_prefix(BASE).unwrap_or(&url).to_string();
        match (method.as_str(), path.as_str()) {
            ("GET", "/dir") => (
                200,
                vec![],
                json(serde_json::json!({
                    "newNonce": format!("{BASE}/nonce"), "newAccount": format!("{BASE}/acct"), "newOrder": format!("{BASE}/order"),
                    "meta": {"profiles": {"shortlived": "about six days"}}
                })),
            ),
            ("HEAD", "/nonce") => (200, vec![], vec![]),
            ("POST", "/acct") => {
                self.0.lock().unwrap().accounts += 1;
                (201, vec![("location", format!("{BASE}/acct/1"))], json(serde_json::json!({"status": "valid"})))
            }
            ("POST", "/order") => {
                let mut c = self.0.lock().unwrap();
                c.profile = payload["profile"].as_str().map(str::to_string);
                (c.valid, c.issued) = (false, None);
                (201, vec![("location", format!("{BASE}/order/1"))], json(Self::order(&c)))
            }
            ("POST", "/authz/1") => {
                let c = self.0.lock().unwrap();
                let st = if c.valid { "valid" } else { "pending" };
                (
                    200,
                    vec![],
                    json(serde_json::json!({
                        "status": st, "identifier": {"type": "dns", "value": "org.example.test"},
                        "challenges": [{"type": "http-01", "url": format!("{BASE}/chall/1"), "token": c.token, "status": st}]
                    })),
                )
            }
            ("POST", "/chall/1") => {
                let (port, token) = {
                    let c = self.0.lock().unwrap();
                    (c.http_port, c.token.clone())
                };
                // validate like a CA would: fetch the token from the host's HTTP port
                let mut tcp = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
                tcp.write_all(
                    format!("GET /.well-known/acme-challenge/{token} HTTP/1.1\r\nHost: org.example.test\r\nConnection: close\r\n\r\n").as_bytes(),
                )
                .await
                .unwrap();
                let mut got = vec![];
                tcp.read_to_end(&mut got).await.unwrap();
                let got = String::from_utf8_lossy(&got).to_string();
                let body = got.split("\r\n\r\n").nth(1).unwrap_or("");
                let ok = body.starts_with(&format!("{token}.")) && body.len() > token.len() + 20;
                self.0.lock().unwrap().valid = ok;
                let st = if ok { "valid" } else { "invalid" };
                (200, vec![], json(serde_json::json!({"type": "http-01", "url": format!("{BASE}/chall/1"), "token": token, "status": st})))
            }
            ("POST", "/order/1") => (200, vec![], json(Self::order(&self.0.lock().unwrap()))),
            ("POST", "/finalize/1") => {
                let der = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload["csr"].as_str().unwrap_or("")).unwrap();
                let mut csr = rcgen::CertificateSigningRequestParams::from_der(&der.into()).unwrap();
                let now = time::OffsetDateTime::now_utc();
                csr.params.not_before = now - time::Duration::minutes(1);
                csr.params.not_after = now + time::Duration::days(6);
                let mut c = self.0.lock().unwrap();
                let leaf = csr.signed_by(&c.issuer).unwrap();
                c.issued = Some(format!("{}{}", leaf.pem(), c.ca_pem));
                (200, vec![], json(Self::order(&c)))
            }
            ("POST", "/cert/1") => (
                200,
                vec![("content-type", "application/pem-certificate-chain".into())],
                self.0.lock().unwrap().issued.clone().unwrap_or_default().into_bytes(),
            ),
            _ => (404, vec![], json(serde_json::json!({"type": "urn:ietf:params:acme:error:malformed", "detail": format!("no {method} {path}")}))),
        }
    }
}

impl HttpClient for MockCa {
    fn request(
        &self,
        req: Request<BodyWrapper<Bytes>>,
    ) -> Pin<Box<dyn Future<Output = std::result::Result<BytesResponse, instant_acme::Error>> + Send>> {
        let me = self.clone();
        Box::pin(async move {
            let (parts, body) = req.into_parts();
            let body = body.collect().await.map(|b| b.to_bytes().to_vec()).unwrap_or_default();
            let (status, headers, out) = me.answer(parts.method.to_string(), parts.uri.to_string(), body).await;
            let mut r = Response::builder().status(status).header("replay-nonce", "nonce-1").header("content-type", "application/json");
            for (k, v) in headers {
                r = r.header(k, v);
            }
            Ok(BytesResponse::from(r.body(Full::new(Bytes::from(out))).unwrap()))
        })
    }
}

#[test]
fn acme_gets_a_short_lived_certificate_through_http_01_and_keeps_it() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("m.db");
    let s = Store::open(&db, Some(&[3u8; 32])).unwrap();
    hosted_lab(&s);
    let ca = MockCa::new();
    let ca2 = ca.clone();
    let http: AcmeHttp = Arc::new(move || Box::new(ca2.clone()) as Box<dyn HttpClient>);
    let acme =
        Acme { directory: format!("{BASE}/dir"), domain: "org.example.test".into(), email: "lab@example.test".into(), profile: "shortlived".into() };
    let o = HostOptions { acme: Some(acme.clone()), ..opts() };
    let host = Host::start_with(s, o.clone(), Some(http.clone())).unwrap();
    assert_eq!(host.cert().source, "self-signed", "until the CA answers");
    ca.0.lock().unwrap().http_port = host.http_addr.unwrap().port();
    let info = host.renew().unwrap().unwrap();
    assert_eq!(info.source, "acme");
    assert_eq!(info.names, vec!["org.example.test".to_string()]);
    assert!(info.issuer.contains("Mock CA") && !info.needs_renewal(now_ms()));
    assert_eq!(ca.0.lock().unwrap().profile.as_deref(), Some("shortlived"));
    // the new certificate is served at once
    let r = send(host.addr, true, "GET", "/", &[], vec![]);
    let (_, x) = x509_parser::parse_x509_certificate(&r.cert).unwrap();
    assert!(x.issuer().to_string().contains("Mock CA"));
    assert_eq!(r.headers["strict-transport-security"], "max-age=86400");
    // the challenge token is gone once used
    assert!(host.state.challenges.read().unwrap().is_empty());
    host.with_store(|s| {
        assert!(s.actions()?.iter().any(|a| a.kind == "org.host.cert"));
        assert!(stored_cert(s, "org.example.test")?.is_some());
        assert!(stored_cert(s, "other.test")?.is_none());
        Ok(())
    })
    .unwrap();
    host.stop();
    // a restart serves the kept certificate and reuses the account
    let s = Store::open(&db, Some(&[3u8; 32])).unwrap();
    let host = Host::start_with(s, o, Some(http)).unwrap();
    assert_eq!(host.cert().source, "acme");
    ca.0.lock().unwrap().http_port = host.http_addr.unwrap().port();
    host.renew().unwrap();
    assert_eq!(ca.0.lock().unwrap().accounts, 1, "the account is kept in the store");
    host.stop();
}

#[test]
fn acme_needs_a_domain_and_an_email() {
    let mut p = crate::prefs::AcmePrefs::default();
    assert_eq!(Acme::from_prefs(&p).unwrap(), None, "off by default");
    p.enabled = true;
    assert!(Acme::from_prefs(&p).is_err());
    p.domain = "lab.example.org".into();
    p.email = "me@example.org".into();
    let a = Acme::from_prefs(&p).unwrap().unwrap();
    assert_eq!((a.directory.as_str(), a.profile.as_str()), ("https://acme-v02.api.letsencrypt.org/directory", "shortlived"));
    let h = crate::prefs::HostingPrefs::default();
    assert_eq!((h.bind.as_str(), h.port, h.acme.enabled), ("127.0.0.1", 8443, false));
}

#[test]
fn the_host_streams_reflection_and_lists_executables() {
    use crate::org::reflect::{FileDescriptorProto, ReflectionRequest, ReflectionResponse};
    let s = Store::open_in_memory().unwrap();
    hosted_lab(&s);
    let host = Host::start(s, opts()).unwrap();
    let hs = vec![("content-type", "application/grpc".to_string()), ("te", "trailers".to_string())];
    let mut body = grpc_frame(&ReflectionRequest { list_services: Some(String::new()), ..Default::default() }.encode_to_vec());
    body.extend(grpc_frame(
        &ReflectionRequest { file_by_filename: Some(crate::org::exec::COMMON_PROTO.into()), ..Default::default() }.encode_to_vec(),
    ));
    let r = send(host.addr, true, "POST", "/grpc.reflection.v1.ServerReflection/ServerReflectionInfo", &hs, body);
    assert_eq!(grpc_status(&r), "0");
    let n = u32::from_be_bytes([r.body[1], r.body[2], r.body[3], r.body[4]]) as usize;
    let first = ReflectionResponse::decode(&r.body[5..5 + n]).unwrap();
    assert!(first.list_services_response.unwrap().service.iter().any(|s| s.name == crate::org::exec::LIST_SERVICE));
    let second = ReflectionResponse::decode(grpc_unframe(&r.body[5 + n..]).unwrap()).unwrap();
    let f = FileDescriptorProto::decode(second.file_descriptor_response.unwrap().file_descriptor_proto[0].as_slice()).unwrap();
    assert_eq!(f.package.as_deref(), Some("muninn.code.v1"));
    // guests run nothing
    let msg = crate::org::exec::pb::ListRequest { org: "lab".into() }.encode_to_vec();
    let r = send(host.addr, true, "POST", "/muninn.code.v1.Executables/List", &hs, grpc_frame(&msg));
    assert_eq!(grpc_status(&r), "0");
    assert!(crate::org::exec::pb::ExecutableList::decode(grpc_unframe(&r.body).unwrap()).unwrap().executables.is_empty());
    // a public org, so the package resolves; a guest lacks code.run
    let r = send(host.addr, true, "POST", "/muninn.code.v1.o.lab.Sma/Run", &hs, grpc_frame(&[]));
    assert_eq!(grpc_status(&r), "7");
    host.stop();
}

#[test]
fn a_gated_private_site_lets_members_sign_in_a_browser() {
    let s = Store::open_in_memory().unwrap();
    let (_, members) = hosted_lab(&s);
    let o = s.find_org("lab").unwrap();
    edit(&s, "cli", &Actor::Owner, &o, &OrgChange { visibility: Some(Visibility::Private), gated: Some(true), ..Default::default() }).unwrap();
    let host = Host::start(s, opts()).unwrap();
    let get = |p: &str, hs: &[(&str, String)]| send(host.addr, true, "GET", p, hs, vec![]);
    // guests: the landing, sign-in and join pages only
    let land = get("/o/lab/", &[]);
    assert!(land.status == 200 && String::from_utf8_lossy(&land.body).contains("private"));
    assert_eq!(get("/o/lab/sign-in", &[]).status, 200);
    assert_eq!(get("/o/lab/join", &[]).status, 200);
    let p = format!("/o/lab/a/{members}.json");
    assert_eq!(get(&p, &[]).status, 404);
    assert_eq!(get("/o/lab/api/artifacts", &[]).status, 404);
    // a one-time signed sign-in link opens a session cookie for this org's pages
    let auth = sign_with(&Key32(KEY), "x-ben", "GET", "/o/lab/sign-in", b"", now_ms());
    let link = format!("/o/lab/sign-in?auth={}", query_escape(&auth));
    let r = get(&link, &[]);
    assert_eq!((r.status, r.headers[header::LOCATION].to_str().unwrap()), (303, "/o/lab/"));
    let set = r.headers[header::SET_COOKIE].to_str().unwrap().to_string();
    assert!(set.contains("HttpOnly") && set.contains("Secure") && set.contains("SameSite=Strict") && set.contains("Path=/o/lab/"), "{set}");
    let cookie = set.split(';').next().unwrap().to_string();
    assert_eq!(get(&link, &[]).status, 401, "the link works once");
    let ok = get(&p, &[("cookie", cookie.clone())]);
    assert!(ok.status == 200 && String::from_utf8_lossy(&ok.body).contains("members only"));
    assert_eq!(get(&p, &[("cookie", "muninn_session=00".into())]).status, 404);
    assert_eq!(get("/o/lab/sign-out", &[("cookie", cookie.clone())]).status, 303);
    assert_eq!(get(&p, &[("cookie", cookie)]).status, 404, "signed out");
    host.stop();
}

#[test]
fn queries_and_join_links_round_trip() {
    let v = "Muninn exchange=x-ben, ts=1, sig=ab+/=";
    assert_eq!(query_value(&format!("a=1&auth={}", query_escape(v)), "auth").as_deref(), Some(v));
    assert_eq!(join_url("https://h:1/", "lab", "muninn://join/AB"), "https://h:1/o/lab/join#muninn%3A%2F%2Fjoin%2FAB");
    assert_eq!(session_cookie(Some("a=b; muninn_session=cafe")), Some("cafe"));
}
