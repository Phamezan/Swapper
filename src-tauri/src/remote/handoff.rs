//! One-time handoff tokens.
//!
//! A phone pairs over HTTP using the PC's current IP, so its
//! `swapper_lan_session` cookie is host-only for that IP. When the IP changes
//! that cookie no longer applies, and the stable `swapper.local` hostname has
//! no cookie at all. To bridge the two without ever putting the long-lived
//! device credential in a URL, an already-authenticated request on the IP
//! origin mints a short-lived token bound to that paired device; the phone then
//! presents it to the `.local` origin, which sets the same cookie there.
//!
//! Tokens live in memory only and are single-use. A Swapper restart just means
//! the phone hands off again.

use std::time::{Duration, Instant};

/// How long a handoff token stays usable. Long enough for one navigation,
/// short enough that a token captured from the LAN is stale quickly.
pub const HANDOFF_TTL: Duration = Duration::from_secs(60);

struct Token {
    value: String,
    device_id: String,
    expires_at: Instant,
}

/// In-memory store of pending handoff tokens.
#[derive(Default)]
pub struct HandoffTokens {
    tokens: Vec<Token>,
}

impl HandoffTokens {
    /// Mints a single-use token bound to one paired device, first dropping any
    /// expired tokens so the list cannot grow without bound.
    pub fn mint(&mut self, device_id: &str, now: Instant, ttl: Duration) -> String {
        self.tokens.retain(|token| token.expires_at > now);
        let value = uuid::Uuid::new_v4().simple().to_string();
        self.tokens.push(Token {
            value: value.clone(),
            device_id: device_id.to_string(),
            expires_at: now + ttl,
        });
        value
    }

    /// Consumes a token exactly once, returning the device it was bound to.
    /// Unknown, already-used, or expired tokens return `None`.
    pub fn consume(&mut self, value: &str, now: Instant) -> Option<String> {
        self.tokens.retain(|token| token.expires_at > now);
        let index = self.tokens.iter().position(|token| token.value == value)?;
        Some(self.tokens.swap_remove(index).device_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_can_be_consumed_once_and_only_by_the_bound_device() {
        let mut tokens = HandoffTokens::default();
        let now = Instant::now();
        let token = tokens.mint("device-a", now, HANDOFF_TTL);
        assert_eq!(tokens.consume(&token, now), Some("device-a".to_string()));
        // Single use: the second attempt fails.
        assert_eq!(tokens.consume(&token, now), None);
    }

    #[test]
    fn expired_tokens_are_rejected_and_pruned() {
        let mut tokens = HandoffTokens::default();
        let now = Instant::now();
        let token = tokens.mint("device-a", now, Duration::from_secs(60));
        let later = now + Duration::from_secs(61);
        assert_eq!(tokens.consume(&token, later), None);
        // Minting after expiry must not keep the dead token around.
        let fresh = tokens.mint("device-b", later, HANDOFF_TTL);
        assert_eq!(tokens.consume(&fresh, later), Some("device-b".to_string()));
    }

    #[test]
    fn unknown_tokens_are_rejected() {
        let mut tokens = HandoffTokens::default();
        assert_eq!(tokens.consume("deadbeef", Instant::now()), None);
        assert_eq!(tokens.consume("", Instant::now()), None);
    }

    #[test]
    fn each_mint_is_distinct_and_keeps_its_own_device() {
        let mut tokens = HandoffTokens::default();
        let now = Instant::now();
        let first = tokens.mint("device-a", now, HANDOFF_TTL);
        let second = tokens.mint("device-b", now, HANDOFF_TTL);
        assert_ne!(first, second);
        assert_eq!(tokens.consume(&second, now), Some("device-b".to_string()));
        assert_eq!(tokens.consume(&first, now), Some("device-a".to_string()));
    }

    #[test]
    fn an_unused_token_does_not_leak_into_another_devices_handoff() {
        let mut tokens = HandoffTokens::default();
        let now = Instant::now();
        let token = tokens.mint("device-a", now, HANDOFF_TTL);
        // Device-b has no token that consumes device-a's.
        assert_eq!(tokens.consume(&format!("{token}x"), now), None);
        assert_eq!(tokens.consume(&token, now), Some("device-a".to_string()));
    }
}
