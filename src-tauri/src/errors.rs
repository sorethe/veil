use serde::{Deserialize, Serialize};

/// Stable error codes for the command boundary. The frontend may switch on
/// these; the original human-readable message is always preserved verbatim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    NotReady,
    Validation,
    Permission,
    Engine,
    Network,
    Io,
    Unsupported,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: ErrorCode,
    pub message: String,
    pub detail: Option<String>,
}

impl CommandError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn not_ready(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotReady, message)
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Validation, message)
    }

    pub fn permission(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Permission, message)
    }

    pub fn engine(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Engine, message)
    }

    pub fn network(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Network, message)
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Io, message)
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unsupported, message)
    }

    pub fn unknown(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unknown, message)
    }

    /// Best-effort classification for internal `Result<T, String>` values at
    /// the command boundary. The message is never modified.
    pub fn classify(message: impl Into<String>) -> Self {
        let message = message.into();
        let lower = message.to_ascii_lowercase();
        let code = if contains_any(
            &lower,
            &["not connected", "not ready", "still running", "already running"],
        ) {
            ErrorCode::NotReady
        } else if contains_any(
            &lower,
            &[
                "permission denied",
                "not permitted",
                "polkit",
                "pkexec",
                "authentication",
                "must run as root",
                "eacces",
            ],
        ) {
            ErrorCode::Permission
        } else if contains_any(
            &lower,
            &[
                "unsupported",
                "not supported",
                "not installed",
                "not available",
                "binary not found",
                "no file manager",
                "clipboard",
            ],
        ) {
            ErrorCode::Unsupported
        } else if contains_any(
            &lower,
            &[
                "request failed",
                "network",
                "timed out",
                "timeout",
                "http 4",
                "http 5",
                "http 3",
                "http 2",
            ],
        ) {
            ErrorCode::Network
        } else if contains_any(
            &lower,
            &[
                "sqlite",
                "database",
                "no such file",
                "os error",
                "i/o",
                "create ",
                "write ",
                "read ",
            ],
        ) {
            ErrorCode::Io
        } else if contains_any(
            &lower,
            &[
                "invalid",
                "validation",
                "config check",
                "no outbounds",
                "must ",
                "missing",
                "no profiles",
                "not found",
                "empty",
                "expected",
                "decode failed",
                "no qr",
            ],
        ) {
            ErrorCode::Validation
        } else if contains_any(&lower, &["engine is not", "sing-box exited", "clash api"]) {
            ErrorCode::Engine
        } else {
            ErrorCode::Unknown
        };
        Self::new(code, message)
    }

    /// Classification for engine lifecycle errors where the string originates
    /// in `engine.rs` rather than a generic I/O or database call.
    pub fn from_engine(message: impl Into<String>) -> Self {
        let message = message.into();
        let lower = message.to_ascii_lowercase();
        let code = if contains_any(
            &lower,
            &["still running", "already running", "not connected", "not ready"],
        ) {
            ErrorCode::NotReady
        } else if contains_any(
            &lower,
            &["permission", "polkit", "pkexec", "authentication", "root"],
        ) {
            ErrorCode::Permission
        } else if contains_any(
            &lower,
            &[
                "config check",
                "no profiles",
                "no profile",
                "unknown mode",
                "must contain",
                "must be",
                "invalid json",
                "invalid mode",
                "no outbounds",
                "profile not found",
            ],
        ) {
            ErrorCode::Validation
        } else if contains_any(
            &lower,
            &[
                "not installed",
                "not found",
                "unsupported",
                "unavailable",
                "is missing",
            ],
        ) {
            ErrorCode::Unsupported
        } else {
            ErrorCode::Engine
        };
        Self::new(code, message)
    }
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

impl From<String> for CommandError {
    fn from(message: String) -> Self {
        Self::classify(message)
    }
}

impl From<&str> for CommandError {
    fn from(message: &str) -> Self {
        Self::classify(message)
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{:?}] {}", self.code, self.message)
    }
}

impl std::error::Error for CommandError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_contract_envelope() {
        let error = CommandError::validation("config check failed").with_detail("stderr tail");
        let value = serde_json::to_value(&error).unwrap();
        assert_eq!(value["code"], "validation");
        assert_eq!(value["message"], "config check failed");
        assert_eq!(value["detail"], "stderr tail");
    }

    #[test]
    fn codes_are_stable_kebab_case() {
        for (code, text) in [
            (ErrorCode::NotReady, "not-ready"),
            (ErrorCode::Validation, "validation"),
            (ErrorCode::Permission, "permission"),
            (ErrorCode::Engine, "engine"),
            (ErrorCode::Network, "network"),
            (ErrorCode::Io, "io"),
            (ErrorCode::Unsupported, "unsupported"),
            (ErrorCode::Unknown, "unknown"),
        ] {
            assert_eq!(serde_json::to_value(code).unwrap(), text);
        }
    }

    #[test]
    fn classifies_without_losing_message() {
        let message = "Clash API did not become ready within 12s: connection refused";
        let error = CommandError::classify(message);
        assert_eq!(error.message, message);
        assert_eq!(error.code, ErrorCode::Engine);
        let error = CommandError::classify("engine is not connected");
        assert_eq!(error.code, ErrorCode::NotReady);
        let error = CommandError::classify("pkexec authentication was cancelled");
        assert_eq!(error.code, ErrorCode::Permission);
        let error = CommandError::classify("unsupported link scheme 'ftp'");
        assert_eq!(error.code, ErrorCode::Unsupported);
    }

    #[test]
    fn engine_validation_is_not_misclassified_as_engine() {
        let error = CommandError::from_engine(
            "sing-box config check failed for 'Home': FATAL decode config",
        );
        assert_eq!(error.code, ErrorCode::Validation);
        let error = CommandError::from_engine("veil-helper is not installed");
        assert_eq!(error.code, ErrorCode::Unsupported);
        let error = CommandError::from_engine("sing-box exited during startup");
        assert_eq!(error.code, ErrorCode::Engine);
    }
}
