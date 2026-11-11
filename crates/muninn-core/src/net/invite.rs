//! Muninn as weirdnet-net's invitation host: exchange records in the store, Muninn's wire
//! messages, and Muninn's decisions (org applications and memberships, review links, how
//! many offers may wait). The automation itself is `weirdnet_net::invite`.
use super::proto::{Wire, WireResp};
use crate::error::{Error, Result};
use crate::store::{Exchange, ExchangeStatus, NewAction, NewExchange, Store};
use libp2p::PeerId;
use serde_json::Value;
use weirdnet_net::Derived;
use weirdnet_net::exchange::Offer;
use weirdnet_net::invite::{Ask, Host, NewRec, Rec, Role, Said, Say, Status};
use zeroize::Zeroizing;

/// At most this many incoming offers are kept.
pub(crate) const MAX_INCOMING: usize = 50;

pub(crate) fn status(s: ExchangeStatus) -> Status {
    match s {
        ExchangeStatus::Offered => Status::Offered,
        ExchangeStatus::Incoming => Status::Incoming,
        ExchangeStatus::Complete => Status::Complete,
        ExchangeStatus::Verified => Status::Verified,
        ExchangeStatus::Revoked => Status::Revoked,
    }
}

pub(crate) fn exchange_status(s: Status) -> ExchangeStatus {
    match s {
        Status::Offered => ExchangeStatus::Offered,
        Status::Incoming => ExchangeStatus::Incoming,
        Status::Complete => ExchangeStatus::Complete,
        Status::Verified => ExchangeStatus::Verified,
        Status::Revoked => ExchangeStatus::Revoked,
    }
}

impl Rec for Exchange {
    fn id(&self) -> &str {
        &self.id
    }
    fn role(&self) -> Role {
        if self.role == "offerer" { Role::Offerer } else { Role::Answerer }
    }
    fn status(&self) -> Status {
        status(self.status)
    }
    fn peer_id(&self) -> Option<&str> {
        self.peer_id.as_deref()
    }
    fn label(&self) -> &str {
        &self.label
    }
    fn peer_label(&self) -> &str {
        &self.peer_label
    }
    fn scope(&self) -> &Value {
        &self.scope
    }
    fn offer(&self) -> &str {
        &self.offer
    }
    fn answer(&self) -> Option<&str> {
        self.answer.as_deref()
    }
    fn expires_at(&self) -> Option<i64> {
        self.expires_at
    }
    fn revoked_by(&self) -> Option<&str> {
        self.revoked_by.as_deref()
    }
    fn fingerprint(&self) -> Option<&str> {
        self.fingerprint.as_deref()
    }
    fn title(&self) -> String {
        Exchange::title(self)
    }
    fn purpose(&self) -> String {
        super::auto::purpose(self)
    }
}

impl Host for Store {
    type Error = Error;
    type Rec = Exchange;
    fn records(&self) -> Result<Vec<Exchange>> {
        self.exchanges()
    }
    fn get(&self, id: &str) -> Result<Option<Exchange>> {
        self.exchange(id)
    }
    fn insert(&self, n: &NewRec, secret: Option<&[u8]>) -> Result<Exchange> {
        self.insert_exchange(
            &NewExchange {
                id: n.id.to_string(),
                label: n.label.to_string(),
                role: n.role.as_str().into(),
                status: Some(exchange_status(n.status)),
                peer_id: Some(n.peer_id.to_string()),
                peer_label: n.peer_label.to_string(),
                scope: n.scope.clone(),
                offer: n.offer.to_string(),
                expires_at: Some(n.expires_at),
            },
            secret,
        )
    }
    fn complete(&self, id: &str, peer: &str, peer_label: &str, answer: &str, d: &Derived) -> Result<Exchange> {
        self.complete_exchange(id, peer, peer_label, answer, &d.key, &d.short_code, &d.fingerprint)
    }
    fn secret(&self, id: &str) -> Result<Option<Zeroizing<Vec<u8>>>> {
        self.exchange_secret(id)
    }
    fn set_status(&self, id: &str, st: Status, by: &str) -> Result<Exchange> {
        self.set_exchange_status(id, exchange_status(st), Some(by))
    }
    fn relabel(&self, id: &str, label: &str) -> Result<()> {
        self.label_exchange(id, label)
    }
    fn answer_delivered(&self, id: &str) -> Result<()> {
        self.mark_answer_delivered(id)
    }
    fn undelivered(&self) -> Result<Vec<Exchange>> {
        self.undelivered_answers()
    }
    fn saw(&self, peer: &str, label: Option<&str>, addrs: &[String], agent: Option<&str>, source: &str) -> Result<()> {
        self.upsert_net_peer(peer, label, addrs, None, agent, source).map(|_| ())
    }
    fn sightings(&self, peer: &str) -> Vec<String> {
        self.net_peer(peer).ok().flatten().map(|p| p.addrs).unwrap_or_default()
    }
    fn peer_label(&self, peer: &str) -> Option<String> {
        self.net_peer(peer).ok().flatten().map(|p| p.label)
    }
    fn named(&self, label: &str) -> Vec<String> {
        self.net_peers().map(|v| v.into_iter().filter(|p| p.label.eq_ignore_ascii_case(label)).map(|p| p.peer_id).collect()).unwrap_or_default()
    }
    fn find_peer(&self, key: &str) -> Result<String> {
        self.find_net_peer(key).map(|p| p.peer_id)
    }
    fn note(&self, origin: &str, kind: &str, summary: String, params: Value) {
        self.note_action(&NewAction::new(kind, origin, summary).params(params));
    }
    fn admit(&self, o: &Offer) -> Result<()> {
        crate::org::net::check_application(self, o)
    }
    fn kept(&self, o: &Offer, x: &Exchange) -> Result<Option<String>> {
        crate::org::net::applied(self, o, x)
    }
    fn completed(&self, x: &Exchange) -> Result<()> {
        crate::org::net::answered(self, x)?;
        crate::review::net::answered(self, x)
    }
    fn poll(&self, x: &Exchange, from: &PeerId) -> Option<Result<(String, Option<String>)>> {
        let kind = x.scope.get("kind").and_then(|k| k.as_str()).unwrap_or("");
        let how = x.scope.get("as").and_then(|k| k.as_str()).unwrap_or("");
        (kind == "organization" && how == "apply").then(|| crate::org::net::poll(self, from, &x.id))
    }
    fn max_incoming(&self) -> usize {
        MAX_INCOMING
    }
    fn not_ours(&self) -> String {
        "that node does not speak Muninn".into()
    }
}

impl Ask for Wire {
    fn hello() -> Self {
        Wire::Hello
    }
    fn connect(token: String) -> Self {
        Wire::Connect { token }
    }
    fn offer(token: String) -> Self {
        Wire::Offer { token }
    }
    fn answer(token: String) -> Self {
        Wire::Answer { token }
    }
    fn poll(exchange: String) -> Self {
        Wire::Poll { exchange }
    }
}

impl Say for WireResp {
    fn said(self) -> Said {
        match self {
            WireResp::Hello { label, agent, .. } => Said::Hello { label, agent },
            WireResp::Ok { msg } => Said::Ok(msg),
            WireResp::Answered { token } => Said::Answered(token),
            WireResp::Poll { status, answer } => Said::Poll { status, answer },
            WireResp::Error { msg } => Said::Error(msg),
            _ => Said::Other,
        }
    }
}
