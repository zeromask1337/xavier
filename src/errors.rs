use std::fmt;

use serde::Serialize;

/// Stable, machine-readable error categories.
///
/// These values are part of the CLI contract: agents may match on them and
/// should never need to parse English error messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Bad flags, invalid values, or conflicting options.
    Usage,
    /// `XAI_API_KEY` is missing or empty.
    MissingApiKey,
    /// The API rejected the credentials (HTTP 401).
    Auth,
    /// The credentials are valid but lack access (HTTP 403).
    Forbidden,
    /// The model or endpoint does not exist (HTTP 404).
    NotFound,
    /// The API rate limited the request (HTTP 429).
    RateLimited,
    /// The API failed internally (HTTP 5xx).
    Server,
    /// The request timed out before a response arrived.
    Timeout,
    /// The API could not be reached.
    Network,
    /// The response was malformed, unparseable, or otherwise unusable.
    InvalidResponse,
    /// The model stopped before finishing its work.
    Incomplete,
    /// The model returned a placeholder instead of real results.
    Placeholder,
    /// The configured spending budget was exhausted. Exit code 4.
    BudgetExceeded,
    /// The input was empty, such as a missing query.
    Empty,
}

impl ErrorCode {
    pub const ALL: [ErrorCode; 14] = [
        Self::Usage,
        Self::MissingApiKey,
        Self::Auth,
        Self::Forbidden,
        Self::NotFound,
        Self::RateLimited,
        Self::Server,
        Self::Timeout,
        Self::Network,
        Self::InvalidResponse,
        Self::Incomplete,
        Self::Placeholder,
        Self::BudgetExceeded,
        Self::Empty,
    ];

    /// Stable string form used by `--json-errors` and `capabilities --json`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Usage => "usage",
            Self::MissingApiKey => "missing_api_key",
            Self::Auth => "auth",
            Self::Forbidden => "forbidden",
            Self::NotFound => "not_found",
            Self::RateLimited => "rate_limited",
            Self::Server => "server",
            Self::Timeout => "timeout",
            Self::Network => "network",
            Self::InvalidResponse => "invalid_response",
            Self::Incomplete => "incomplete",
            Self::Placeholder => "placeholder",
            Self::BudgetExceeded => "budget_exceeded",
            Self::Empty => "empty",
        }
    }

    /// Whether retrying the same request may succeed without changing it.
    pub fn retryable(self) -> bool {
        matches!(
            self,
            Self::RateLimited
                | Self::Server
                | Self::Timeout
                | Self::Network
                | Self::Incomplete
                | Self::Placeholder
        )
    }

    /// Map an HTTP status to the closest stable code.
    pub fn from_http_status(status: u16) -> Self {
        match status {
            401 => Self::Auth,
            403 => Self::Forbidden,
            404 => Self::NotFound,
            429 => Self::RateLimited,
            500..=599 => Self::Server,
            _ => Self::InvalidResponse,
        }
    }

    /// Exit code paired with this error: 1 runtime, 2 usage/configuration,
    /// 4 budget exhaustion.
    pub fn exit_code(self) -> i32 {
        match self {
            Self::Usage | Self::MissingApiKey | Self::Empty => 2,
            Self::BudgetExceeded => 4,
            _ => 1,
        }
    }
}

/// A CLI error with a stable code, the exit code, and optional remediation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppError {
    code: ErrorCode,
    message: String,
    hint: Option<String>,
    http_status: Option<u16>,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hint: None,
            http_status: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn with_http_status(mut self, status: u16) -> Self {
        self.http_status = Some(status);
        self
    }

    pub fn usage(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Usage, message)
    }

    pub fn missing_api_key() -> Self {
        Self::new(ErrorCode::MissingApiKey, "XAI_API_KEY is not set.")
            .with_hint("Run with: doppler run -- xavier \"your query\" or export XAI_API_KEY=...")
    }

    /// A response failure that is not tied to a specific HTTP status.
    pub fn runtime(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidResponse, message)
    }

    pub fn runtime_with_hint(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::runtime(message).with_hint(hint)
    }

    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// Preserve the established exit codes: 0 success, 1 runtime, 2 usage.
    /// Exit code 4 is reserved for budget exhaustion.
    pub fn exit_code(&self) -> i32 {
        self.code.exit_code()
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn hint(&self) -> Option<&str> {
        self.hint.as_deref()
    }

    pub fn http_status(&self) -> Option<u16> {
        self.http_status
    }

    pub fn retryable(&self) -> bool {
        self.code.retryable()
    }

    pub fn is_usage(&self) -> bool {
        matches!(self.code, ErrorCode::Usage)
    }

    /// One-line JSON object for stderr when `--json-errors` is enabled.
    pub fn to_json_string(&self) -> String {
        #[derive(Serialize)]
        struct Envelope<'a> {
            error: Body<'a>,
        }

        #[derive(Serialize)]
        struct Body<'a> {
            code: &'static str,
            message: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            hint: Option<&'a str>,
            retryable: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            http_status: Option<u16>,
        }

        let envelope = Envelope {
            error: Body {
                code: self.code.as_str(),
                message: &self.message,
                hint: self.hint(),
                retryable: self.retryable(),
                http_status: self.http_status,
            },
        };
        serde_json::to_string(&envelope).expect("error object is serializable")
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for AppError {}

pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_has_a_stable_string_exit_code_and_retry_policy() {
        let expected = [
            (ErrorCode::Usage, "usage", false, 2),
            (ErrorCode::MissingApiKey, "missing_api_key", false, 2),
            (ErrorCode::Auth, "auth", false, 1),
            (ErrorCode::Forbidden, "forbidden", false, 1),
            (ErrorCode::NotFound, "not_found", false, 1),
            (ErrorCode::RateLimited, "rate_limited", true, 1),
            (ErrorCode::Server, "server", true, 1),
            (ErrorCode::Timeout, "timeout", true, 1),
            (ErrorCode::Network, "network", true, 1),
            (ErrorCode::InvalidResponse, "invalid_response", false, 1),
            (ErrorCode::Incomplete, "incomplete", true, 1),
            (ErrorCode::Placeholder, "placeholder", true, 1),
            (ErrorCode::BudgetExceeded, "budget_exceeded", false, 4),
            (ErrorCode::Empty, "empty", false, 2),
        ];

        assert_eq!(expected.len(), ErrorCode::ALL.len());
        for (code, name, retryable, exit_code) in expected {
            assert!(ErrorCode::ALL.contains(&code));
            assert_eq!(code.as_str(), name);
            assert_eq!(code.retryable(), retryable, "{name}");
            assert_eq!(AppError::new(code, "boom").exit_code(), exit_code);
        }
    }

    #[test]
    fn maps_http_statuses_to_codes() {
        assert_eq!(ErrorCode::from_http_status(401), ErrorCode::Auth);
        assert_eq!(ErrorCode::from_http_status(403), ErrorCode::Forbidden);
        assert_eq!(ErrorCode::from_http_status(404), ErrorCode::NotFound);
        assert_eq!(ErrorCode::from_http_status(429), ErrorCode::RateLimited);
        assert_eq!(ErrorCode::from_http_status(500), ErrorCode::Server);
        assert_eq!(ErrorCode::from_http_status(503), ErrorCode::Server);
        assert_eq!(ErrorCode::from_http_status(400), ErrorCode::InvalidResponse);
        assert_eq!(ErrorCode::from_http_status(418), ErrorCode::InvalidResponse);
    }

    #[test]
    fn json_error_omits_absent_optional_fields() {
        let parsed: serde_json::Value =
            serde_json::from_str(&AppError::usage("bad flag").to_json_string()).unwrap();
        assert_eq!(parsed["error"]["code"], "usage");
        assert_eq!(parsed["error"]["message"], "bad flag");
        assert_eq!(parsed["error"]["retryable"], false);
        assert!(parsed["error"].get("hint").is_none());
        assert!(parsed["error"].get("http_status").is_none());
    }

    #[test]
    fn json_error_includes_hint_and_http_status() {
        let error = AppError::new(ErrorCode::RateLimited, "slow down")
            .with_hint("Wait and retry.")
            .with_http_status(429);
        let parsed: serde_json::Value = serde_json::from_str(&error.to_json_string()).unwrap();
        assert_eq!(
            parsed,
            serde_json::json!({
                "error": {
                    "code": "rate_limited",
                    "message": "slow down",
                    "hint": "Wait and retry.",
                    "retryable": true,
                    "http_status": 429
                }
            })
        );
    }

    #[test]
    fn missing_api_key_uses_exit_code_2() {
        let error = AppError::missing_api_key();
        assert_eq!(error.code(), ErrorCode::MissingApiKey);
        assert_eq!(error.exit_code(), 2);
        assert!(!error.retryable());
        assert!(error.hint().is_some());
    }
}
