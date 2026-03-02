//! Exit codes and error classification for the CLI.
//!
//! Every error that reaches `main` is mapped to one of these codes so that
//! callers can programmatically distinguish failure categories.

use std::fmt;

/// Exit codes returned by the process.
///
/// | Code | Meaning                                              |
/// |------|------------------------------------------------------|
/// |  0   | Success                                              |
/// |  1   | Authentication error (OAuth failed, bad credentials) |
/// |  2   | Invalid input (bad URI, malformed JSON, missing arg) |
/// |  3   | Network error (timeout, DNS, connection refused)     |
/// |  4   | API error (not found, region-restricted, rate limit) |
/// |  5   | Audio key error (decryption key request failed)      |
/// |  6   | Audio download error (stream interrupted, I/O)       |
/// |  7   | Serialization error (failed to produce JSON output)  |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    Success = 0,
    AuthError = 1,
    InvalidInput = 2,
    NetworkError = 3,
    ApiError = 4,
    AudioKeyError = 5,
    AudioDownloadError = 6,
    SerializationError = 7,
}

impl From<ExitCode> for i32 {
    fn from(code: ExitCode) -> i32 {
        code as i32
    }
}

/// A CLI error carrying a human-readable message and an [`ExitCode`].
#[derive(Debug)]
pub struct CliError {
    pub code: ExitCode,
    pub message: String,
    /// Optional underlying cause for logging.
    pub source: Option<anyhow::Error>,
}

impl CliError {
    pub fn new(code: ExitCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source(code: ExitCode, message: impl Into<String>, source: anyhow::Error) -> Self {
        Self {
            code,
            message: message.into(),
            source: Some(source),
        }
    }

    /// Emit the error as a JSON object to stderr and return the exit code.
    pub fn emit_and_exit_code(&self) -> i32 {
        let json = serde_json::json!({
            "error": {
                "code": self.code as u8,
                "message": self.message,
            }
        });
        eprintln!("{}", json);
        if let Some(ref src) = self.source {
            tracing::debug!("Underlying error: {src:#}");
        }
        self.code.into()
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[exit {}] {}", self.code as u8, self.message)
    }
}

impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}
