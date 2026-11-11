/// What went wrong. Applications map these onto their own errors; the messages say what
/// happened without naming the application.
#[derive(Debug, thiserror::Error)]
pub enum NetError {
    /// Bad input: a token, an offer or answer, a scope, a card, a request.
    #[error("invalid {what}: {msg}")]
    Invalid { what: &'static str, msg: String },
    /// A signature, key or sealed message that does not check out.
    #[error("crypto: {0}")]
    Crypto(String),
    /// The node, a transport, a peer that can't be reached, a timeout.
    #[error("network: {0}")]
    Network(String),
    /// From the keys crate, unchanged.
    #[error(transparent)]
    Keys(#[from] weirdnet_keys::KeysError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Something in the host's records that should not happen (a damaged or missing entry).
    #[error("{0}")]
    Other(String),
}

impl NetError {
    pub fn invalid(what: &'static str, msg: impl Into<String>) -> Self {
        NetError::Invalid { what, msg: msg.into() }
    }
    pub(crate) fn net(e: impl std::fmt::Display) -> Self {
        NetError::Network(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, NetError>;
