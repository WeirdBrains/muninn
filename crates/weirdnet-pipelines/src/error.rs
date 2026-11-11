/// A graph, node, edge or frame input that is not valid.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PipelineError {
    #[error("invalid {what}: {msg}")]
    Invalid { what: &'static str, msg: String },
}

impl PipelineError {
    pub fn invalid(what: &'static str, msg: impl Into<String>) -> Self {
        PipelineError::Invalid { what, msg: msg.into() }
    }
}

pub type Result<T> = std::result::Result<T, PipelineError>;

pub(crate) fn invalid(msg: impl Into<String>) -> PipelineError {
    PipelineError::invalid("pipeline", msg)
}
