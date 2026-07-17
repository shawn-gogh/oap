use sqlx::PgPool;

use crate::{
    db::managed_agents::{id, now_ms},
    errors::GatewayError,
};

use super::schema::CloudEventReceiptRow;

pub struct RecordCloudEvent<'a> {
    pub direction: &'a str,
    pub session_id: &'a str,
    pub cloud_event_id: &'a str,
    pub cloud_event_source: &'a str,
    pub cloud_event_type: &'a str,
    pub subject: Option<&'a str>,
    pub data_digest: &'a str,
    pub canonical_event_key: &'a str,
    pub actor_user_id: &'a str,
}

pub struct RecordedCloudEvent {
    pub row: CloudEventReceiptRow,
    pub duplicate: bool,
}

pub async fn record(
    pool: &PgPool,
    input: RecordCloudEvent<'_>,
) -> Result<RecordedCloudEvent, GatewayError> {
    let now = now_ms();
    let row = sqlx::query_as::<_, CloudEventReceiptRow>(
        r#"
        INSERT INTO "LiteLLM_CloudEventReceiptsTable" (
          id, direction, session_id, cloud_event_id, cloud_event_source,
          cloud_event_type, subject, data_digest, canonical_event_key,
          actor_user_id, first_seen_at, last_seen_at, delivery_count
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11, 1)
        ON CONFLICT (direction, session_id, cloud_event_source, cloud_event_id)
        DO UPDATE SET
          last_seen_at = EXCLUDED.last_seen_at,
          delivery_count = "LiteLLM_CloudEventReceiptsTable".delivery_count + 1
        WHERE "LiteLLM_CloudEventReceiptsTable".data_digest = EXCLUDED.data_digest
        RETURNING *
        "#,
    )
    .bind(id("cereceipt"))
    .bind(input.direction)
    .bind(input.session_id)
    .bind(input.cloud_event_id)
    .bind(input.cloud_event_source)
    .bind(input.cloud_event_type)
    .bind(input.subject)
    .bind(input.data_digest)
    .bind(input.canonical_event_key)
    .bind(input.actor_user_id)
    .bind(now)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)?;

    let Some(row) = row else {
        return Err(GatewayError::BadRequest(
            "CloudEvent source/id 已存在，但数据摘要不同。".to_owned(),
        ));
    };
    Ok(RecordedCloudEvent {
        duplicate: row.delivery_count > 1,
        row,
    })
}
