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
    review_period_days: i32,
}

#[derive(Debug, Deserialize)]
pub struct UpdateGovernanceSettings {
    separation_of_duties: bool,
    review_period_days: Option<i32>,
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
        review_period_days: repository::review_period_days(pool).await?,
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
    let review_period_days = match input.review_period_days {
        Some(days) => repository::set_review_period_days(pool, days, &auth.user_id).await?,
        None => repository::review_period_days(pool).await?,
    };
    crate::db::managed_agents::audit::record(
        pool,
        &auth.user_id,
        "governance.settings.updated",
        "gateway",
        "separation_of_duties",
        serde_json::json!({
            "separation_of_duties": enabled,
            "review_period_days": review_period_days,
        }),
    )
    .await?;
    Ok(Json(GovernanceSettings {
        separation_of_duties: enabled,
        review_period_days,
    }))
}
