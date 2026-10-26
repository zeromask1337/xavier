use std::fmt;

/// A CLI error with the exit code and optional remediation shown to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    /// Bad flags, invalid values, or missing configuration. Exit code 2.
    Usage(String),
    /// API, network, or response error. Exit code 1.
    Runtime {
        message: String,
        hint: Option<String>,
    },
}

impl AppError {
    pub fn usage(message: impl Into<String>) -> Self {
        Self::Usage(message.into())
    }

    pub fn runtime(message: impl Into<String>) -> Self {
        Self::Runtime {
            message: message.into(),
            hint: None,
        }
    }

    pub fn runtime_with_hint(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::Runtime {
            message: message.into(),
            hint: Some(hint.into()),
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Usage(_) => 2,
            Self::Runtime { .. } => 1,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Usage(message) => message,
            Self::Runtime { message, .. } => message,
        }
    }

    pub fn hint(&self) -> Option<&str> {
        match self {
            Self::Usage(_) => None,
            Self::Runtime { hint, .. } => hint.as_deref(),
        }
    }

    pub fn is_usage(&self) -> bool {
        matches!(self, Self::Usage(_))
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for AppError {}

pub type Result<T> = std::result::Result<T, AppError>;
