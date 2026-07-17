use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::db::managed_agents::{identity_mappings, users};

use super::{
    types::{ExternalIdentity, PlatformIdentity},
    AdapterError, AdapterFuture, IdentityAdapter,
};

#[derive(Clone)]
pub struct DatabaseIdentityAdapter {
    pool: PgPool,
}

impl DatabaseIdentityAdapter {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl IdentityAdapter for DatabaseIdentityAdapter {
    fn resolve<'a>(
        &'a self,
        identity: &'a ExternalIdentity,
    ) -> AdapterFuture<'a, PlatformIdentity> {
        Box::pin(async move {
            let issuer = identity.issuer.trim();
            let subject = identity.subject.trim();
            if issuer.is_empty() || subject.is_empty() {
                return Err(AdapterError::InvalidConfiguration(
                    "external identity issuer and subject are required".to_owned(),
                ));
            }
            let audience = identity.audience.as_deref().unwrap_or("").trim();
            let digest = claims_digest(&identity.claims)?;
            let mapping = identity_mappings::repository::observe(
                &self.pool,
                issuer,
                subject,
                audience,
                &digest,
                json!({
                    "issuer": issuer,
                    "audience": audience,
                    "source": "database_identity_adapter",
                }),
            )
            .await
            .map_err(|error| AdapterError::Storage(error.to_string()))?;

            match mapping.status.as_str() {
                "pending" => return Err(AdapterError::UnmappedIdentity(mapping.id)),
                "blocked" => return Err(AdapterError::BlockedIdentity(mapping.id)),
                "active" => {}
                status => {
                    return Err(AdapterError::Storage(format!(
                        "identity mapping {} has invalid status {status}",
                        mapping.id
                    )))
                }
            }

            let user_id = mapping.platform_user_id.clone().ok_or_else(|| {
                AdapterError::Storage(format!(
                    "active identity mapping {} has no user",
                    mapping.id
                ))
            })?;
            let user = users::repository::find(&self.pool, &user_id)
                .await
                .map_err(|error| AdapterError::Storage(error.to_string()))?;
            if !user.is_some_and(|user| user.is_active()) {
                return Err(AdapterError::BlockedIdentity(mapping.id));
            }

            Ok(PlatformIdentity {
                user_id,
                agent_id: mapping.platform_agent_id,
                groups: Vec::new(),
                mapping_evidence: json!({
                    "mapping_id": mapping.id,
                    "issuer": mapping.issuer,
                    "audience": mapping.audience,
                }),
            })
        })
    }
}

fn claims_digest(claims: &serde_json::Value) -> Result<String, AdapterError> {
    let encoded = serde_json::to_vec(claims)
        .map_err(|error| AdapterError::Decode(format!("identity claims: {error}")))?;
    let mut hash = Sha256::new();
    hash.update(encoded);
    Ok(format!("sha256:{:x}", hash.finalize()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::claims_digest;

    #[test]
    fn claims_digest_is_stable_and_does_not_expose_claims() {
        let claims = json!({"email": "private@example.test", "role": "operator"});
        let first = claims_digest(&claims).expect("digest");
        let second = claims_digest(&claims).expect("digest");
        assert_eq!(first, second);
        assert!(first.starts_with("sha256:"));
        assert!(!first.contains("private@example.test"));
    }
}
