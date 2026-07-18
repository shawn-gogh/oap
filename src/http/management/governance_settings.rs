use std::sync::Arc;

use axum::{extract::State, http::HeaderMap, Json};
use serde::{Deserialize, Serialize};

use crate::{
    db::managed_agents::settings::repository,
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

#[derive(Debug, Serialize)]
pub struct GovernanceSettings {
    separation_of_duties: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateGovernanceSettings {
    separation_of_duties: bool,
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<GovernanceSettings>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    if !auth.is_admin {
        return Err(GatewayError::Forbidden);
    }
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    Ok(Json(GovernanceSettings {
        separation_of_duties: repository::enforce_separation_of_duties(pool).await?,
    }))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<UpdateGovernanceSettings>,
) -> Result<Json<GovernanceSettings>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    if !auth.is_admin {
        return Err(GatewayError::Forbidden);
    }
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let enabled =
        repository::set_separation_of_duties(pool, input.separation_of_duties, &auth.user_id)
            .await?;
    crate::db::managed_agents::audit::record(
        pool,
        &auth.user_id,
        "governance.settings.updated",
        "gateway",
        "separation_of_duties",
        serde_json::json!({ "enabled": enabled }),
    )
    .await?;
    Ok(Json(GovernanceSettings {
        separation_of_duties: enabled,
    }))
}
