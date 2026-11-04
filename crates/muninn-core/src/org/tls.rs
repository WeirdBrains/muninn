//! Org TLS (2.16): each org may have its own domains and its own certificate, served by
//! name (SNI) from the one site port. Where an org's certificate comes from, in order:
//! 1. one you supply (`mnnn org cert set ORG --cert FILE --key FILE`), kept in the
//!    encrypted store (the key is never logged or written to a file);
//! 2. Let's Encrypt over ACME for its domains, when ACME is on for the org;
//! 3. a self-signed one for its domains, made when the host starts.
//!
//! Settings keys (all in the encrypted store): `org.domains.ID` (a JSON list),
//! `org.acme.ID` (`{enabled, email}`), `org.cert.user.ID` and `org.cert.acme.ID` (a
//! chain and key), `org.cert.retry.ID` (ACME failures, for backoff), `org.cert.renew.ID`
//! (a renewal asked for by the expiry watcher).
use super::host::{CertInfo, CertPem, cert_info};
use crate::error::{Error, Result};
use crate::store::{Org, Store, now_ms};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub const USER: &str = "user";
pub const ACME: &str = "acme";
pub const SELF_SIGNED: &str = "self-signed";
pub const DAY_MS: i64 = 24 * 60 * 60 * 1000;

fn key(what: &str, org: &Org) -> String {
    format!("org.{what}.{}", org.id)
}

/// A host name an org may be served at: lower case, letters, digits, `-` and dots, at
/// least one dot (or `localhost`), no wildcards.
pub fn check_domain(d: &str) -> Result<String> {
    let d = d.trim().trim_end_matches('.').to_ascii_lowercase();
    let label_ok = |l: &str| {
        !l.is_empty() && l.len() <= 63 && !l.starts_with('-') && !l.ends_with('-') && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    };
    if d.len() > 253
        || !(d.contains('.') || d == "localhost")
        || !d.split('.').all(label_ok)
        || d.split('.').all(|l| l.chars().all(|c| c.is_ascii_digit()))
    {
        return Err(Error::invalid("domain", format!("`{d}` is not a host name (like lab.example.org)")));
    }
    Ok(d)
}

pub fn domains(s: &Store, org: &Org) -> Result<Vec<String>> {
    Ok(s.setting(&key("domains", org))?.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default())
}

fn set_domains(s: &Store, org: &Org, v: &[String]) -> Result<()> {
    let j = serde_json::to_string(v)?;
    s.set_setting(&key("domains", org), if v.is_empty() { None } else { Some(&j) })
}

/// Which org a host name belongs to.
pub fn org_for_domain(s: &Store, domain: &str) -> Result<Option<Org>> {
    let d = domain.trim().trim_end_matches('.').to_ascii_lowercase();
    for o in s.orgs()? {
        if domains(s, &o)?.contains(&d) {
            return Ok(Some(o));
        }
    }
    Ok(None)
}

/// Add a domain (refused when another org has it). Returns false when it was there.
pub fn add_domain(s: &Store, org: &Org, domain: &str) -> Result<bool> {
    let d = check_domain(domain)?;
    if let Some(o) = org_for_domain(s, &d)? {
        if o.id == org.id {
            return Ok(false);
        }
        return Err(Error::invalid("domain", format!("`{d}` belongs to {} already", o.name)));
    }
    let mut v = domains(s, org)?;
    v.push(d);
    set_domains(s, org, &v)?;
    crate::rules::ensure_cert_watch(s)?;
    Ok(true)
}

pub fn remove_domain(s: &Store, org: &Org, domain: &str) -> Result<bool> {
    let d = domain.trim().trim_end_matches('.').to_ascii_lowercase();
    let mut v = domains(s, org)?;
    let n = v.len();
    v.retain(|x| *x != d);
    set_domains(s, org, &v)?;
    Ok(v.len() != n)
}

/// ACME for one org: on or off, and the contact address (empty: the node's).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OrgAcme {
    pub enabled: bool,
    #[serde(default)]
    pub email: String,
}

pub fn acme(s: &Store, org: &Org) -> Result<OrgAcme> {
    Ok(s.setting(&key("acme", org))?.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default())
}

pub fn set_acme(s: &Store, org: &Org, a: &OrgAcme) -> Result<()> {
    s.set_setting(&key("acme", org), Some(&serde_json::to_string(a)?))?;
    if a.enabled {
        crate::rules::ensure_cert_watch(s)?;
    }
    Ok(())
}

fn stored(s: &Store, what: &str, org: &Org) -> Result<Option<CertPem>> {
    let Some(v) = s.setting(&key(what, org))?.map(Zeroizing::new) else { return Ok(None) };
    Ok(serde_json::from_str(&v).ok())
}

fn save(s: &Store, what: &str, org: &Org, c: Option<&CertPem>) -> Result<()> {
    match c {
        Some(c) => {
            let v = Zeroizing::new(serde_json::to_string(c)?);
            s.set_setting(&key(what, org), Some(&v))
        }
        None => s.set_setting(&key(what, org), None),
    }
}

/// Check a certificate and key you supply: PEM, the key matches the leaf, not expired.
pub fn check_user_cert(c: &CertPem) -> Result<CertInfo> {
    let info = cert_info(c, USER)?;
    super::host::check_pair(c)?;
    if info.not_after <= now_ms() {
        return Err(Error::invalid("certificate", "it has expired"));
    }
    Ok(info)
}

/// Keep your own certificate for the org (it takes priority over ACME and self-signed).
pub fn set_user_cert(s: &Store, org: &Org, c: &CertPem) -> Result<CertInfo> {
    let info = check_user_cert(c)?;
    save(s, "cert.user", org, Some(c))?;
    s.set_setting(&key("cert.notified", org), None)?;
    crate::rules::ensure_cert_watch(s)?;
    Ok(info)
}

pub fn clear_user_cert(s: &Store, org: &Org) -> Result<bool> {
    let had = stored(s, "cert.user", org)?.is_some();
    save(s, "cert.user", org, None)?;
    Ok(had)
}

pub fn user_cert(s: &Store, org: &Org) -> Result<Option<CertPem>> {
    stored(s, "cert.user", org)
}

pub fn acme_cert(s: &Store, org: &Org) -> Result<Option<CertPem>> {
    stored(s, "cert.acme", org)
}

pub fn save_acme_cert(s: &Store, org: &Org, c: &CertPem) -> Result<()> {
    save(s, "cert.acme", org, Some(c))
}

/// The certificate an org gets now, by priority (`None`: self-signed, made by the host).
pub fn choose(s: &Store, org: &Org, now: i64) -> Result<Option<(CertPem, &'static str)>> {
    if let Some(c) = user_cert(s, org)?
        && cert_info(&c, USER).is_ok_and(|i| i.not_after > now)
    {
        return Ok(Some((c, USER)));
    }
    if acme(s, org)?.enabled
        && let Some(c) = acme_cert(s, org)?
        && cert_info(&c, ACME).is_ok_and(|i| i.not_after > now)
    {
        return Ok(Some((c, ACME)));
    }
    Ok(None)
}

/// ACME failures for an org: attempts so far and when to try next (1 h, doubling, at most a day).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Retry {
    pub attempts: u32,
    pub next_at: i64,
    #[serde(default)]
    pub error: String,
}

pub fn retry(s: &Store, org: &Org) -> Result<Retry> {
    Ok(s.setting(&key("cert.retry", org))?.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default())
}

/// Note a failed ACME attempt; returns the backoff state.
pub fn failed(s: &Store, org: &Org, error: &str, now: i64) -> Result<Retry> {
    let mut r = retry(s, org)?;
    r.attempts += 1;
    let wait = (60 * 60 * 1000i64).saturating_mul(1 << (r.attempts - 1).min(5)).min(DAY_MS);
    r.next_at = now + wait;
    r.error = error.to_string();
    s.set_setting(&key("cert.retry", org), Some(&serde_json::to_string(&r)?))?;
    Ok(r)
}

pub fn succeeded(s: &Store, org: &Org) -> Result<()> {
    s.set_setting(&key("cert.retry", org), None)?;
    s.set_setting(&key("cert.renew", org), None)
}

/// Ask the running host to renew the org's ACME certificate on its next check.
pub fn request_renewal(s: &Store, org: &Org) -> Result<()> {
    s.set_setting(&key("cert.renew", org), Some(&now_ms().to_string()))
}

pub fn renewal_requested(s: &Store, org: &Org) -> Result<bool> {
    Ok(s.setting(&key("cert.renew", org))?.is_some())
}

/// Which expiry notice (days left) was last sent for a user certificate, and for which one.
fn notified(s: &Store, org: &Org) -> Result<Option<(i64, i64)>> {
    Ok(s.setting(&key("cert.notified", org))?.and_then(|v| serde_json::from_str(&v).ok()))
}

fn set_notified(s: &Store, org: &Org, not_after: i64, days: i64) -> Result<()> {
    s.set_setting(&key("cert.notified", org), Some(&serde_json::to_string(&(not_after, days))?))
}

/// One org's certificate state (no secrets).
#[derive(Debug, Clone, Serialize)]
pub struct OrgCert {
    pub org: String,
    pub slug: String,
    pub domains: Vec<String>,
    /// `user`, `acme` or `self-signed` (what is served, or would be).
    pub source: String,
    pub cert: Option<CertInfo>,
    pub acme: OrgAcme,
    pub retry: Option<Retry>,
}

impl OrgCert {
    pub fn days_left(&self, now: i64) -> Option<i64> {
        self.cert.as_ref().map(|c| (c.not_after - now).div_euclid(DAY_MS))
    }
}

pub fn status(s: &Store, org: &Org) -> Result<OrgCert> {
    let now = now_ms();
    let (source, cert) = match choose(s, org, now)? {
        Some((c, src)) => (src.to_string(), Some(cert_info(&c, src)?)),
        None => (SELF_SIGNED.to_string(), None),
    };
    let r = retry(s, org)?;
    Ok(OrgCert {
        org: org.name.clone(),
        slug: org.slug.clone(),
        domains: domains(s, org)?,
        source,
        cert,
        acme: acme(s, org)?,
        retry: (r.attempts > 0).then_some(r),
    })
}

/// Every org with domains or a certificate of its own.
pub fn all(s: &Store) -> Result<Vec<OrgCert>> {
    let mut v = vec![];
    for o in s.orgs()? {
        let c = status(s, &o)?;
        if !c.domains.is_empty() || c.cert.is_some() || c.acme.enabled {
            v.push(c);
        }
    }
    Ok(v)
}

/// The expiry watcher's check (the `cert-watch` then-action of the built-in
/// `cert-expiry-watch` rule set): ACME certificates with fewer than `renew_days` left are
/// renewed (asked of the running host; failures retry with backoff and notify); your own
/// certificates notify once at each of `notify_days` and on expiry. Returns what it did.
pub fn watch(s: &Store, origin: &str, renew_days: i64, notify_days: &[i64], host_running: bool) -> Result<Vec<String>> {
    let now = now_ms();
    let mut out = vec![];
    for o in s.orgs()? {
        let st = status(s, &o)?;
        if st.domains.is_empty() && st.cert.is_none() {
            continue;
        }
        if let Some(c) = user_cert(s, &o)?
            && let Ok(i) = cert_info(&c, USER)
        {
            let left = (i.not_after - now).div_euclid(DAY_MS);
            let due = if i.not_after <= now { Some(0) } else { notify_days.iter().copied().filter(|d| left < *d).min() };
            if let Some(d) = due {
                let last = notified(s, &o)?;
                if last.is_none_or(|(na, days)| na != i.not_after || d < days) {
                    let msg = if i.not_after <= now {
                        format!("the certificate for {} has expired; renew it with your provider and `mnnn org cert set {}`", o.name, o.slug)
                    } else {
                        format!(
                            "the certificate for {} expires in {left} days; renew it with your provider and `mnnn org cert set {}`",
                            o.name, o.slug
                        )
                    };
                    notify(s, origin, "warn", "cert.expiry", &o, &msg, serde_json::json!({ "days_left": left, "source": USER }));
                    set_notified(s, &o, i.not_after, d)?;
                    out.push(msg);
                }
            }
            continue;
        }
        if !st.acme.enabled || st.domains.is_empty() {
            continue;
        }
        let left = st.days_left(now);
        if left.is_some_and(|l| l >= renew_days) && st.source == ACME {
            continue;
        }
        if host_running {
            request_renewal(s, &o)?;
            out.push(format!("asked the host to renew {}'s certificate", o.name));
        } else {
            let msg = format!(
                "{}'s certificate {}; start the host (`mnnn org host start`) so it can renew through ACME",
                o.name,
                match left {
                    Some(l) => format!("has {l} days left"),
                    None => "is self-signed".into(),
                }
            );
            notify(s, origin, "warn", "cert.expiry", &o, &msg, serde_json::json!({ "days_left": left, "source": ACME }));
            out.push(msg);
        }
    }
    Ok(out)
}

/// Record a notification (kind `notify`): shown in the history and the TUI, and a
/// rule trigger like any action (`--on notify --if "about = cert.expiry"`).
///
/// Minimal until Phase 23 (Notifications) adds the header bell and node broadcasts: the
/// params (`level`, `about`, `message`, plus the org and details) are the shape that
/// phase reads, so nothing recorded now needs migrating.
pub fn notify(s: &Store, origin: &str, level: &str, about: &str, org: &Org, message: &str, extra: serde_json::Value) {
    let mut p = serde_json::json!({ "level": level, "about": about, "org": org.id, "slug": org.slug, "message": message });
    if let (Some(m), Some(e)) = (p.as_object_mut(), extra.as_object()) {
        m.extend(e.clone());
    }
    s.note_action(&crate::store::NewAction::new("notify", origin, message.to_string()).params(p));
}
