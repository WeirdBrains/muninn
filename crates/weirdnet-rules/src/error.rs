/// A rule, condition, pattern or schedule that does not parse.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RulesError {
    #[error("invalid {what}: {msg}")]
    Invalid { what: &'static str, msg: String },
}

impl RulesError {
    pub fn invalid(what: &'static str, msg: impl Into<String>) -> Self {
        RulesError::Invalid { what, msg: msg.into() }
    }
}

pub type Result<T> = std::result::Result<T, RulesError>;
