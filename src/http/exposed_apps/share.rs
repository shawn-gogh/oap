use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::errors::GatewayError;

type HmacSha256 = Hmac<Sha256>;

const KEY_CONTEXT: &str = "exposed-app-share-v1";

/// Cookie holding a verified share token, scoped to one app via its Path.
pub fn share_cookie_name(app_id: &str) -> String {
    format!("lap_app_{app_id}")
}

/// Token format: `{app_id}.{exp_ms}.{share_version}.{hex hmac}`. The signed
/// payload binds all three fields, so a token is only valid for its own app,
/// until its expiry, and while the app's share_version is unchanged
/// (bumping the version revokes every outstanding token).
pub fn sign_token(master_key: &str, app_id: &str, exp_ms: i64, share_version: i32) -> String {
    let payload = format!("{app_id}.{exp_ms}.{share_version}");
    let mut mac = mac(master_key);
    mac.update(payload.as_bytes());
    let signature = mac.finalize().into_bytes();
    format!("{payload}.{}", hex(&signature))
}

pub fn verify_token(
    master_key: &str,
    token: &str,
    app_id: &str,
    share_version: i32,
    now_ms: i64,
) -> bool {
    let Some((payload, signature_hex)) = token.rsplit_once('.') else {
        return false;
    };
    let mut parts = payload.split('.');
    let (Some(token_app), Some(exp), Some(version)) = (parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    if parts.next().is_some() || token_app != app_id {
        return false;
    }
    let (Ok(exp), Ok(version)) = (exp.parse::<i64>(), version.parse::<i32>()) else {
        return false;
    };
    if exp <= now_ms || version != share_version {
        return false;
    }
    let Some(signature) = unhex(signature_hex) else {
        return false;
    };
    let mut mac = mac(master_key);
    mac.update(payload.as_bytes());
    mac.verify_slice(&signature).is_ok()
}

pub fn require_master_key(configured: Option<&str>) -> Result<&str, GatewayError> {
    configured.ok_or_else(|| {
        GatewayError::BadRequest(
            "share links require a configured master_key for signing".to_owned(),
        )
    })
}

fn mac(master_key: &str) -> HmacSha256 {
    let mut key_mac =
        HmacSha256::new_from_slice(master_key.as_bytes()).expect("hmac accepts any key length");
    key_mac.update(KEY_CONTEXT.as_bytes());
    let derived = key_mac.finalize().into_bytes();
    HmacSha256::new_from_slice(&derived).expect("hmac accepts any key length")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(input: &str) -> Option<Vec<u8>> {
    if !input.len().is_multiple_of(2) {
        return None;
    }
    (0..input.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&input[index..index + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_valid_token() {
        let token = sign_token("sk-test", "app_1", 10_000, 0);
        assert!(verify_token("sk-test", &token, "app_1", 0, 5_000));
    }

    #[test]
    fn rejects_expired_token() {
        let token = sign_token("sk-test", "app_1", 10_000, 0);
        assert!(!verify_token("sk-test", &token, "app_1", 0, 10_000));
    }

    #[test]
    fn rejects_bumped_share_version() {
        let token = sign_token("sk-test", "app_1", 10_000, 0);
        assert!(!verify_token("sk-test", &token, "app_1", 1, 5_000));
    }

    #[test]
    fn rejects_other_app_and_tampered_signature() {
        let token = sign_token("sk-test", "app_1", 10_000, 0);
        assert!(!verify_token("sk-test", &token, "app_2", 0, 5_000));
        let mut tampered = token.clone();
        tampered.pop();
        tampered.push('0');
        // May equal original last char; flip deterministically instead.
        let flipped = if tampered == token {
            format!("{}1", &token[..token.len() - 1])
        } else {
            tampered
        };
        assert!(!verify_token("sk-test", &flipped, "app_1", 0, 5_000));
    }

    #[test]
    fn rejects_wrong_key() {
        let token = sign_token("sk-test", "app_1", 10_000, 0);
        assert!(!verify_token("sk-other", &token, "app_1", 0, 5_000));
    }
}
