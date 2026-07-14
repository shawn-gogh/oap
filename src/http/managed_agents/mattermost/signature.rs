use crate::errors::GatewayError;

/// Mattermost Outgoing Webhooks authenticate with a static `token` field in
/// the posted form body (no HMAC signing like Slack) — constant-time compare
/// against the configured verification token is the whole check.
pub(super) fn verify(presented: &str, expected: &str) -> Result<(), GatewayError> {
    if constant_time_eq(presented.as_bytes(), expected.as_bytes()) {
        Ok(())
    } else {
        Err(GatewayError::Unauthorized)
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right.iter())
        .fold(0u8, |acc, (left, right)| acc | (left ^ right))
        == 0
}

#[cfg(test)]
mod tests {
    use super::verify;

    #[test]
    fn accepts_matching_token() {
        assert!(verify("secret", "secret").is_ok());
    }

    #[test]
    fn rejects_mismatched_or_wrong_length_tokens() {
        assert!(verify("secret", "wrong").is_err());
        assert!(verify("secret", "secretlonger").is_err());
        assert!(verify("", "secret").is_err());
    }
}
