//! GraphGateway core domain — token generation, constant-time comparison,
//! and utilities shared by server and client without pulling in HTTP deps.

use rand::Rng;
use subtle::ConstantTimeEq;

/// Generate a cryptographically-random access token.
///
/// Returns a 256-bit hex-encoded string suitable for use as a Bearer token.
pub fn generate_access_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill(&mut bytes);
    hex_encode(&bytes)
}

/// Constant-time comparison of two byte slices using the `subtle` crate.
///
/// This prevents timing side-channels that could leak the token value
/// through ordinary `==` short-circuiting.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.ct_eq(b).into()
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_token_is_random() {
        let a = generate_access_token();
        let b = generate_access_token();
        assert_ne!(a, b);
        assert_eq!(a.len(), 64); // 32 bytes → 64 hex chars
    }

    #[test]
    fn constant_time_eq_true() {
        assert!(constant_time_eq(b"abc", b"abc"));
    }

    #[test]
    fn constant_time_eq_false() {
        assert!(!constant_time_eq(b"abc", b"abd"));
    }

    #[test]
    fn constant_time_eq_different_lengths() {
        assert!(!constant_time_eq(b"ab", b"abc"));
    }
}
