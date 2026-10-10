use serde_json::{Value, json};

/// Short machine-readable error codes (§19.4). Tool failures are tool results
/// with `isError`, never protocol errors, so the model can read and act on them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    StaleRef,
    NotFound,
    Ambiguous,
    Busy,
    NotActionable,
    Timeout,
    CapabilityDenied,
    AppFaulted,
    // Authoring-side codes the inspector list does not cover.
    InvalidArgument,
    /// The tool exists but its backing component is not wired in this build.
    Unavailable,
    /// A path escaped the project root.
    PathDenied,
    Internal,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::StaleRef => "stale_ref",
            ErrorCode::NotFound => "not_found",
            ErrorCode::Ambiguous => "ambiguous",
            ErrorCode::Busy => "busy",
            ErrorCode::NotActionable => "not_actionable",
            ErrorCode::Timeout => "timeout",
            ErrorCode::CapabilityDenied => "capability_denied",
            ErrorCode::AppFaulted => "app_faulted",
            ErrorCode::InvalidArgument => "invalid_argument",
            ErrorCode::Unavailable => "unavailable",
            ErrorCode::PathDenied => "path_denied",
            ErrorCode::Internal => "internal",
        }
    }
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[error("{}: {message}", code.as_str())]
pub struct SurfaceError {
    pub code: ErrorCode,
    pub message: String,
    pub details: Option<Value>,
}

impl SurfaceError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    #[must_use]
    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidArgument, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unavailable, message)
    }

    /// The structured body of an error result: always a JSON object.
    pub fn to_value(&self) -> Value {
        let mut error = json!({ "code": self.code.as_str(), "message": self.message });
        if let Some(details) = &self.details {
            error["details"] = details.clone();
        }
        json!({ "error": error })
    }
}
