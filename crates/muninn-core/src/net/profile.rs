//! Muninn's network identifiers, passed to weirdnet-net everywhere. These are the values
//! v2.23.0 and earlier nodes use, so tokens, links, signatures, certificates, sealed
//! messages and sync pages stay byte-identical (pinned by `compat_tests.rs` and
//! `net_golden_tests.rs`). Switching Muninn to the Weirdnet identifiers is a separate,
//! explicitly approved rollout (a wire break), not part of the extraction.
use weirdnet_net::profile::{Profile, scope_kind};
use weirdnet_net::{NetError, Result};

pub const MUNINN: Profile = Profile {
    name: "muninn",
    protocol: "/muninn/net/1",
    kad: "/muninn/kad/1",
    relay_key: "/muninn/relay/1",
    identify: "/muninn/id/1",
    offer_prefix: "mnnn-x1-offer:",
    answer_prefix: "mnnn-x1-answer:",
    token_mark: "mnnn-x1-",
    join_link: "muninn://join/",
    answer_link: "muninn://answer/",
    link_mark: "muninn://",
    offer_domain: "muninn/net/offer/v1",
    answer_domain: "muninn/net/answer/v1",
    transcript_domain: "muninn/net/exchange/v1",
    key_info: "muninn/net/exchange/v1/key",
    sas_info: "muninn/net/exchange/v1/sas",
    msg_aad: "muninn/net/msg/v1",
    cert: "muninn-node-cert/1",
    revoke: "muninn-node-revoke/1",
    sync_key: "muninn/sync/v1/key",
    sync_aad: "muninn/sync/v1",
    default_port: 4741,
    thread: "muninn-net",
    scopes,
};

/// What an exchange is for: `node` (two nodes), `link` (2.10, a node asking to act for the
/// answering node's user), `organization` (2.7, a membership: `as` is `apply` or `invite`)
/// or `resource` (2.8, a review link: `as: review` names the review). Older nodes refuse
/// the newer kinds instead of misreading them.
fn scopes(scope: &serde_json::Value) -> Result<()> {
    let field = |k: &str| scope.get(k).and_then(|v| v.as_str()).unwrap_or("");
    match scope_kind(scope) {
        "node" | "link" => Ok(()),
        "organization" => {
            let (org, how) = (field("org"), field("as"));
            if org.is_empty() || org.len() > 64 || !matches!(how, "apply" | "invite") {
                return Err(NetError::invalid("exchange", "an organization exchange names the org and whether it applies or invites"));
            }
            Ok(())
        }
        "resource" => {
            let (review, how) = (field("review"), field("as"));
            if review.is_empty() || review.len() > 64 || how != "review" {
                return Err(NetError::invalid("exchange", "a resource exchange names the review it is a link to (newer Muninn?)"));
            }
            Ok(())
        }
        _ => Err(NetError::invalid("exchange", "unknown scope")),
    }
}
