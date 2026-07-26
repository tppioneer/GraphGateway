//! Sidecar startup handshake protocol.
//!
//! The Tauri backend writes a single JSON line to the sidecar's stdin,
//! and expects exactly one JSON "ready" line on stdout.
//!
//! Output rules:
//! - **stdout**: reserved for the sidecar control protocol (ready message).
//! - **stderr**: all application logs via `tracing`.

use std::io::Read;

use graphgateway_types::{PROTOCOL_VERSION, ReadyMessage, StartupConfig};

/// Max startup config size (8 KiB per ADR-001).
const MAX_STARTUP_BYTES: usize = 8 * 1024;

/// Error during the handshake.
#[derive(Debug, thiserror::Error)]
pub(crate) enum HandshakeError {
    #[error("failed to read startup config from stdin: {0}")]
    ReadStdin(#[from] std::io::Error),
    #[error("invalid startup config JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("protocol version mismatch: expected {expected}, got {got}")]
    ProtocolVersion { expected: u32, got: u32 },
    #[error("startup config too large ({len} bytes, max {max})")]
    ConfigTooLarge { len: usize, max: usize },
    #[error("startup config validation failed: {0}")]
    ValidationFailed(String),
    #[error("ready message too large ({len} bytes, max {max})")]
    ReadyTooLarge { len: usize, max: usize },
}

/// Read the startup configuration from stdin.
///
/// Reads **one line**, enforces a size limit, detects truncation/overflow,
/// and validates all fields.
pub(crate) fn read_startup_config() -> Result<StartupConfig, HandshakeError> {
    // Read byte-by-byte up to the limit + 1 (to detect overflow).
    let mut buf = vec![0u8; MAX_STARTUP_BYTES + 1];
    let mut pos = 0;
    let stdin = std::io::stdin();
    let mut handle = stdin.lock();

    while pos < buf.len() {
        let n = handle.read(&mut buf[pos..pos + 1])?;
        if n == 0 {
            break; // EOF
        }
        if buf[pos] == b'\n' {
            // Found the newline — validate what we have.
            let line = std::str::from_utf8(&buf[..pos]).map_err(|e| {
                tracing::error!(error = %e, "startup config is not valid UTF-8");
                HandshakeError::ValidationFailed("startup config must be valid UTF-8".into())
            })?;
            let line = line.trim_end_matches('\r');

            if line.is_empty() {
                return Err(HandshakeError::ReadStdin(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "empty stdin — expected startup config JSON",
                )));
            }

            let config: StartupConfig = serde_json::from_str(line).map_err(|e| {
                // Do NOT log the raw input — it contains the access token.
                tracing::error!(error = %e, "invalid startup config JSON");
                HandshakeError::InvalidJson(e)
            })?;

            // Validate the parsed config.
            validate_startup_config(&config)?;

            return Ok(config);
        }
        pos += 1;
    }

    // EOF without newline, or exceeded limit.
    if pos >= MAX_STARTUP_BYTES {
        return Err(HandshakeError::ConfigTooLarge {
            len: pos,
            max: MAX_STARTUP_BYTES,
        });
    }

    // EOF within limit — accept as a single line (no trailing newline).
    if pos == 0 {
        return Err(HandshakeError::ReadStdin(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "empty stdin — expected startup config JSON",
        )));
    }

    let line = std::str::from_utf8(&buf[..pos]).map_err(|_| {
        HandshakeError::ValidationFailed("startup config must be valid UTF-8".into())
    })?;

    let config: StartupConfig = serde_json::from_str(line).map_err(|e| {
        tracing::error!(error = %e, "invalid startup config JSON");
        HandshakeError::InvalidJson(e)
    })?;

    validate_startup_config(&config)?;

    Ok(config)
}

/// Validate the parsed startup configuration.
fn validate_startup_config(config: &StartupConfig) -> Result<(), HandshakeError> {
    // 1. Protocol version must match.
    if config.protocol_version != PROTOCOL_VERSION {
        return Err(HandshakeError::ProtocolVersion {
            expected: PROTOCOL_VERSION,
            got: config.protocol_version,
        });
    }

    // 2. Token must be exactly 64 hex characters (256 bits).
    if config.access_token.len() != 64 {
        return Err(HandshakeError::ValidationFailed(format!(
            "access_token must be 64 hex chars, got {} chars",
            config.access_token.len()
        )));
    }
    if !config.access_token.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(HandshakeError::ValidationFailed(
            "access_token must contain only hex characters".into(),
        ));
    }

    // 3. parent_pid must be non-zero (0 is allowed for testing ONLY with
    //    an explicit flag; this is not yet implemented).
    if config.parent_pid == 0 {
        return Err(HandshakeError::ValidationFailed(
            "parent_pid must be non-zero".into(),
        ));
    }

    // 4. data_dir must be an absolute path.
    let path = std::path::Path::new(&config.data_dir);
    if !path.is_absolute() {
        return Err(HandshakeError::ValidationFailed(format!(
            "data_dir must be an absolute path, got: {}",
            config.data_dir
        )));
    }

    Ok(())
}

/// Emit the ready message on stdout.
///
/// This is the ONLY output on stdout — all logging goes to stderr via tracing.
pub(crate) fn emit_ready(
    endpoint: &str,
    server_version: &str,
    pid: u32,
) -> Result<(), HandshakeError> {
    let msg = ReadyMessage {
        msg_type: "ready".to_string(),
        protocol_version: PROTOCOL_VERSION,
        pid,
        endpoint: endpoint.to_string(),
        api_version: "v1".to_string(),
        server_version: server_version.to_string(),
    };

    let json = serde_json::to_string(&msg)?;
    let len = json.len();

    const MAX_READY_SIZE: usize = 4096;
    if len > MAX_READY_SIZE {
        return Err(HandshakeError::ReadyTooLarge {
            len,
            max: MAX_READY_SIZE,
        });
    }

    // stdout is the control channel.
    println!("{json}");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_message_format() {
        let msg = ReadyMessage {
            msg_type: "ready".into(),
            protocol_version: 1,
            pid: 1234,
            endpoint: "http://127.0.0.1:9999".into(),
            api_version: "v1".into(),
            server_version: "0.1.0".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ReadyMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.msg_type, "ready");
        assert_eq!(parsed.protocol_version, 1);
    }

    #[test]
    fn startup_config_parse() {
        let json = r#"{"protocol_version":1,"access_token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","parent_pid":42,"data_dir":"C:\\tmp"}"#;
        let cfg: StartupConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.protocol_version, 1);
        assert_eq!(cfg.access_token.len(), 64);
        assert_eq!(cfg.parent_pid, 42);
    }

    #[test]
    fn startup_config_bad_protocol() {
        let json = r#"{"protocol_version":99,"access_token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","parent_pid":42,"data_dir":"C:\\tmp"}"#;
        let cfg: StartupConfig = serde_json::from_str(json).unwrap();
        assert!(validate_startup_config(&cfg).is_err());
    }

    #[test]
    fn startup_config_token_too_short() {
        let json =
            r#"{"protocol_version":1,"access_token":"short","parent_pid":42,"data_dir":"C:\\tmp"}"#;
        let cfg: StartupConfig = serde_json::from_str(json).unwrap();
        assert!(validate_startup_config(&cfg).is_err());
    }

    #[test]
    fn startup_config_token_not_hex() {
        let json = r#"{"protocol_version":1,"access_token":"zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz","parent_pid":42,"data_dir":"C:\\tmp"}"#;
        let cfg: StartupConfig = serde_json::from_str(json).unwrap();
        assert!(validate_startup_config(&cfg).is_err());
    }

    #[test]
    fn startup_config_parent_pid_zero() {
        let json = r#"{"protocol_version":1,"access_token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","parent_pid":0,"data_dir":"C:\\tmp"}"#;
        let cfg: StartupConfig = serde_json::from_str(json).unwrap();
        assert!(validate_startup_config(&cfg).is_err());
    }

    #[test]
    fn startup_config_non_absolute_data_dir() {
        let json = r#"{"protocol_version":1,"access_token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","parent_pid":42,"data_dir":"relative/path"}"#;
        let cfg: StartupConfig = serde_json::from_str(json).unwrap();
        assert!(validate_startup_config(&cfg).is_err());
    }
}
