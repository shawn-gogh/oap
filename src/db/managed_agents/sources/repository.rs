use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::{
    db::managed_agents::{
        governance::AgentGovernanceRow, id, now_ms, registry::schema::ManagedAgentRow,
    },
    errors::GatewayError,
    sdk::agents::canonical::normalize_agent,
};

use super::schema::{
    AgentDriftFindingRow, AgentHealthCheckRow, AgentSourceOverview, AgentSourceSnapshotRow,
    AgentSourceSyncRunRow, CreateSourceConnector, ManagedAgentSourceRow, RuntimeConformanceRow,
    SourceConnectorRow, UpdateSourceConnector,
};

pub async fn find_connector(
    pool: &PgPool,
    owner_id: &str,
    provider: &str,
    endpoint: &str,
) -> Result<Option<SourceConnectorRow>, GatewayError> {
    sqlx::query_as::<_, SourceConnectorRow>(
        r#"
        SELECT * FROM "LiteLLM_AgentSourceConnectorsTable"
        WHERE owner_id = $1 AND provider = $2 AND endpoint = $3
        ORDER BY updated_at DESC
        LIMIT 1
        "#,
    )
    .bind(owner_id)
    .bind(provider)
    .bind(endpoint)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn create_connector(
    pool: &PgPool,
    owner_id: &str,
    input: CreateSourceConnector,
    capabilities: Value,
) -> Result<SourceConnectorRow, GatewayError> {
    let now = now_ms();
    let connector = sqlx::query_as::<_, SourceConnectorRow>(
        r#"
        INSERT INTO "LiteLLM_AgentSourceConnectorsTable" (
          id, owner_id, name, provider, endpoint, credential_name,
          status, capabilities, adapter_id, protocol, protocol_version,
          negotiated_profile, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, 'unknown', $7, $8, $9, $10, $7, $11, $11)
        RETURNING *
        "#,
    )
    .bind(id("connector"))
    .bind(owner_id)
    .bind(input.name)
    .bind(input.provider)
    .bind(input.endpoint)
    .bind(input.credential_name)
    .bind(capabilities)
    .bind(input.adapter_id)
    .bind(input.protocol)
    .bind(input.protocol_version)
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)?;
    sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentSourcesTable" source
        SET connector_id = $1, management_mode = 'federated', updated_at = $2
        FROM "LiteLLM_ManagedAgentGovernanceTable" governance
        WHERE governance.agent_id = source.agent_id
          AND governance.owner_id = $3
          AND governance.source_provider = $4
          AND governance.source_endpoint = $5
          AND source.connector_id IS NULL
        "#,
    )
    .bind(&connector.id)
    .bind(now)
    .bind(owner_id)
    .bind(&connector.provider)
    .bind(&connector.endpoint)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(connector)
}

pub async fn list_connectors(
    pool: &PgPool,
    owner_id: Option<&str>,
) -> Result<Vec<SourceConnectorRow>, GatewayError> {
    sqlx::query_as::<_, SourceConnectorRow>(
        r#"
        SELECT * FROM "LiteLLM_AgentSourceConnectorsTable"
        WHERE $1::TEXT IS NULL OR owner_id = $1
        ORDER BY updated_at DESC
        "#,
    )
    .bind(owner_id)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn get_connector(
    pool: &PgPool,
    connector_id: &str,
) -> Result<Option<SourceConnectorRow>, GatewayError> {
    sqlx::query_as::<_, SourceConnectorRow>(
        r#"SELECT * FROM "LiteLLM_AgentSourceConnectorsTable" WHERE id = $1"#,
    )
    .bind(connector_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn update_connector(
    pool: &PgPool,
    connector_id: &str,
    input: UpdateSourceConnector,
) -> Result<Option<SourceConnectorRow>, GatewayError> {
    sqlx::query_as::<_, SourceConnectorRow>(
        r#"
        UPDATE "LiteLLM_AgentSourceConnectorsTable"
        SET name = COALESCE($2, name), endpoint = COALESCE($3, endpoint),
            credential_name = COALESCE($4, credential_name),
            status = CASE
              WHEN $5::BOOLEAN IS NULL THEN status
              WHEN $5 THEN 'unknown'
              ELSE 'disabled'
            END,
            updated_at = $6
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(connector_id)
    .bind(input.name)
    .bind(input.endpoint)
    .bind(input.credential_name)
    .bind(input.enabled)
    .bind(now_ms())
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn set_connector_test(
    pool: &PgPool,
    connector_id: &str,
    status: &str,
    detail: &str,
) -> Result<SourceConnectorRow, GatewayError> {
    sqlx::query_as::<_, SourceConnectorRow>(
        r#"
        UPDATE "LiteLLM_AgentSourceConnectorsTable"
        SET status = $2, last_test_detail = $3, last_test_at = $4, updated_at = $4
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(connector_id)
    .bind(status)
    .bind(detail)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn delete_connector(pool: &PgPool, connector_id: &str) -> Result<bool, GatewayError> {
    let mut transaction = pool.begin().await.map_err(GatewayError::Database)?;
    sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentSourcesTable"
        SET connector_id = NULL, sync_state = 'detached', next_sync_at = NULL,
            lease_owner = NULL, lease_until = NULL, updated_at = $2
        WHERE connector_id = $1
        "#,
    )
    .bind(connector_id)
    .bind(now_ms())
    .execute(&mut *transaction)
    .await
    .map_err(GatewayError::Database)?;
    let result = sqlx::query(r#"DELETE FROM "LiteLLM_AgentSourceConnectorsTable" WHERE id = $1"#)
        .bind(connector_id)
        .execute(&mut *transaction)
        .await
        .map_err(GatewayError::Database)?;
    transaction.commit().await.map_err(GatewayError::Database)?;
    Ok(result.rows_affected() > 0)
}

pub async fn ensure_source(
    pool: &PgPool,
    governance: &AgentGovernanceRow,
    management_mode: &str,
    connector_id: Option<&str>,
) -> Result<ManagedAgentSourceRow, GatewayError> {
    let now = now_ms();
    sqlx::query_as::<_, ManagedAgentSourceRow>(
        r#"
        INSERT INTO "LiteLLM_ManagedAgentSourcesTable" (
          id, agent_id, connector_id, management_mode, sync_state,
          last_synced_at, created_at, updated_at
        )
        VALUES (
          $1,
          $2,
          COALESCE(
            $3,
            (
              SELECT connector.id
              FROM "LiteLLM_AgentSourceConnectorsTable" connector
              WHERE connector.owner_id = $6
                AND connector.provider = $7
                AND connector.endpoint = $8
              ORDER BY connector.updated_at DESC
              LIMIT 1
            )
          ),
          $4,
          'in_sync',
          $5,
          $5,
          $5
        )
        ON CONFLICT (agent_id) DO UPDATE SET
          connector_id = COALESCE(EXCLUDED.connector_id, "LiteLLM_ManagedAgentSourcesTable".connector_id),
          management_mode = EXCLUDED.management_mode,
          updated_at = EXCLUDED.updated_at
        RETURNING *
        "#,
    )
    .bind(id("src"))
    .bind(&governance.agent_id)
    .bind(connector_id)
    .bind(management_mode)
    .bind(now)
    .bind(&governance.owner_id)
    .bind(&governance.source_provider)
    .bind(&governance.source_endpoint)
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn record_import_snapshot(
    pool: &PgPool,
    source: &ManagedAgentSourceRow,
    agent: &ManagedAgentRow,
    raw_spec: Value,
    digest: &str,
    agent_revision: i32,
    actor: &str,
) -> Result<AgentSourceSnapshotRow, GatewayError> {
    let report = normalize_agent(agent);
    let canonical_spec = serde_json::to_value(&report.spec)
        .map_err(|error| GatewayError::InvalidConfig(error.to_string()))?;
    let issues = serde_json::to_value(&report.issues)
        .map_err(|error| GatewayError::InvalidConfig(error.to_string()))?;
    let version = sqlx::query_scalar::<_, i32>(
        r#"
        SELECT COALESCE(MAX(version), 0) + 1
        FROM "LiteLLM_AgentSourceSnapshotsTable" WHERE source_id = $1
        "#,
    )
    .bind(&source.id)
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)?;
    let snapshot = sqlx::query_as::<_, AgentSourceSnapshotRow>(
        r#"
        INSERT INTO "LiteLLM_AgentSourceSnapshotsTable" (
          id, source_id, version, digest, raw_spec, canonical_spec,
          normalization_issues, agent_revision, created_by, created_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        ON CONFLICT (source_id, digest) DO UPDATE SET
          agent_revision = COALESCE("LiteLLM_AgentSourceSnapshotsTable".agent_revision, EXCLUDED.agent_revision)
        RETURNING *
        "#,
    )
    .bind(id("snap"))
    .bind(&source.id)
    .bind(version)
    .bind(digest)
    .bind(raw_spec)
    .bind(canonical_spec)
    .bind(issues)
    .bind(agent_revision)
    .bind(actor)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)?;
    sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentSourcesTable"
        SET current_snapshot_id = $2, candidate_snapshot_id = NULL,
            sync_state = 'in_sync', missing_count = 0,
            last_synced_at = $3, updated_at = $3
        WHERE id = $1
        "#,
    )
    .bind(&source.id)
    .bind(&snapshot.id)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(snapshot)
}

pub async fn record_candidate_snapshot(
    pool: &PgPool,
    source: &ManagedAgentSourceRow,
    candidate: &ManagedAgentRow,
    raw_spec: Value,
    digest: &str,
    actor: &str,
) -> Result<AgentSourceSnapshotRow, GatewayError> {
    let report = normalize_agent(candidate);
    let canonical_spec = serde_json::to_value(&report.spec)
        .map_err(|error| GatewayError::InvalidConfig(error.to_string()))?;
    let issues = serde_json::to_value(&report.issues)
        .map_err(|error| GatewayError::InvalidConfig(error.to_string()))?;
    let version = sqlx::query_scalar::<_, i32>(
        r#"
        SELECT COALESCE(MAX(version), 0) + 1
        FROM "LiteLLM_AgentSourceSnapshotsTable" WHERE source_id = $1
        "#,
    )
    .bind(&source.id)
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)?;
    let snapshot = sqlx::query_as::<_, AgentSourceSnapshotRow>(
        r#"
        INSERT INTO "LiteLLM_AgentSourceSnapshotsTable" (
          id, source_id, version, digest, raw_spec, canonical_spec,
          normalization_issues, created_by, created_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (source_id, digest) DO UPDATE SET digest = EXCLUDED.digest
        RETURNING *
        "#,
    )
    .bind(id("snap"))
    .bind(&source.id)
    .bind(version)
    .bind(digest)
    .bind(raw_spec)
    .bind(canonical_spec)
    .bind(issues)
    .bind(actor)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)?;
    sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentSourcesTable"
        SET candidate_snapshot_id = $2, sync_state = 'drifted',
            missing_count = 0, last_synced_at = $3,
            next_sync_at = $3 + 300000,
            lease_owner = NULL, lease_until = NULL, updated_at = $3
        WHERE id = $1
        "#,
    )
    .bind(&source.id)
    .bind(&snapshot.id)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(snapshot)
}

pub async fn replace_drift_findings(
    pool: &PgPool,
    source_id: &str,
    snapshot_id: &str,
    findings: &[(String, String, Option<Value>, Option<Value>)],
) -> Result<Vec<AgentDriftFindingRow>, GatewayError> {
    sqlx::query(
        r#"
        UPDATE "LiteLLM_AgentDriftFindingsTable"
        SET resolution = 'superseded', resolved_at = $2
        WHERE source_id = $1 AND resolution = 'open'
        "#,
    )
    .bind(source_id)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    let mut rows = Vec::with_capacity(findings.len());
    for (field_path, risk, previous_value, candidate_value) in findings {
        rows.push(
            sqlx::query_as::<_, AgentDriftFindingRow>(
                r#"
                INSERT INTO "LiteLLM_AgentDriftFindingsTable" (
                  id, source_id, snapshot_id, field_path, risk,
                  previous_value, candidate_value, created_at
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                RETURNING *
                "#,
            )
            .bind(id("drift"))
            .bind(source_id)
            .bind(snapshot_id)
            .bind(field_path)
            .bind(risk)
            .bind(previous_value)
            .bind(candidate_value)
            .bind(now_ms())
            .fetch_one(pool)
            .await
            .map_err(GatewayError::Database)?,
        );
    }
    Ok(rows)
}

pub async fn resolve_candidate(
    pool: &PgPool,
    source_id: &str,
    snapshot_id: &str,
    resolution: &str,
    agent_revision: Option<i32>,
) -> Result<ManagedAgentSourceRow, GatewayError> {
    if let Some(agent_revision) = agent_revision {
        sqlx::query(
            r#"
            UPDATE "LiteLLM_AgentSourceSnapshotsTable"
            SET agent_revision = $2 WHERE id = $1
            "#,
        )
        .bind(snapshot_id)
        .bind(agent_revision)
        .execute(pool)
        .await
        .map_err(GatewayError::Database)?;
    }
    sqlx::query(
        r#"
        UPDATE "LiteLLM_AgentDriftFindingsTable"
        SET resolution = $3, resolved_at = $4
        WHERE source_id = $1 AND snapshot_id = $2 AND resolution = 'open'
        "#,
    )
    .bind(source_id)
    .bind(snapshot_id)
    .bind(resolution)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    sqlx::query_as::<_, ManagedAgentSourceRow>(
        r#"
        UPDATE "LiteLLM_ManagedAgentSourcesTable"
        SET current_snapshot_id = CASE WHEN $3 = 'accepted' THEN $2 ELSE current_snapshot_id END,
            candidate_snapshot_id = NULL, sync_state = 'in_sync',
            updated_at = $4
        WHERE id = $1 RETURNING *
        "#,
    )
    .bind(source_id)
    .bind(snapshot_id)
    .bind(resolution)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn get_source_by_agent(
    pool: &PgPool,
    agent_id: &str,
) -> Result<Option<ManagedAgentSourceRow>, GatewayError> {
    sqlx::query_as::<_, ManagedAgentSourceRow>(
        r#"SELECT * FROM "LiteLLM_ManagedAgentSourcesTable" WHERE agent_id = $1"#,
    )
    .bind(agent_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn get_snapshot(
    pool: &PgPool,
    snapshot_id: Option<&str>,
) -> Result<Option<AgentSourceSnapshotRow>, GatewayError> {
    let Some(snapshot_id) = snapshot_id else {
        return Ok(None);
    };
    sqlx::query_as::<_, AgentSourceSnapshotRow>(
        r#"SELECT * FROM "LiteLLM_AgentSourceSnapshotsTable" WHERE id = $1"#,
    )
    .bind(snapshot_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn overview(
    pool: &PgPool,
    agent_id: &str,
) -> Result<Option<AgentSourceOverview>, GatewayError> {
    let Some(source) = get_source_by_agent(pool, agent_id).await? else {
        return Ok(None);
    };
    let current_snapshot = get_snapshot(pool, source.current_snapshot_id.as_deref()).await?;
    let candidate_snapshot = get_snapshot(pool, source.candidate_snapshot_id.as_deref()).await?;
    let drift_findings = sqlx::query_as::<_, AgentDriftFindingRow>(
        r#"
        SELECT * FROM "LiteLLM_AgentDriftFindingsTable"
        WHERE source_id = $1 ORDER BY created_at DESC LIMIT 100
        "#,
    )
    .bind(&source.id)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)?;
    let recent_sync_runs = sqlx::query_as::<_, AgentSourceSyncRunRow>(
        r#"
        SELECT * FROM "LiteLLM_AgentSourceSyncRunsTable"
        WHERE source_id = $1 ORDER BY started_at DESC LIMIT 20
        "#,
    )
    .bind(&source.id)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)?;
    let recent_health_checks = sqlx::query_as::<_, AgentHealthCheckRow>(
        r#"
        SELECT * FROM "LiteLLM_AgentHealthChecksTable"
        WHERE agent_id = $1 ORDER BY checked_at DESC LIMIT 50
        "#,
    )
    .bind(agent_id)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)?;
    let conformance = sqlx::query_as::<_, RuntimeConformanceRow>(
        r#"SELECT * FROM "LiteLLM_AgentRuntimeConformanceTable" WHERE agent_id = $1"#,
    )
    .bind(agent_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(Some(AgentSourceOverview {
        source,
        current_snapshot,
        candidate_snapshot,
        drift_findings,
        recent_sync_runs,
        recent_health_checks,
        conformance,
    }))
}

pub async fn acquire_sync_lease(
    pool: &PgPool,
    source_id: &str,
    worker_id: &str,
    lease_ms: i64,
) -> Result<bool, GatewayError> {
    let now = now_ms();
    let result = sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentSourcesTable"
        SET lease_owner = $2, lease_until = $3, updated_at = $1
        WHERE id = $4 AND (lease_until IS NULL OR lease_until < $1 OR lease_owner = $2)
        "#,
    )
    .bind(now)
    .bind(worker_id)
    .bind(now.saturating_add(lease_ms))
    .bind(source_id)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(result.rows_affected() == 1)
}

pub async fn start_sync_run(
    pool: &PgPool,
    source: &ManagedAgentSourceRow,
    trigger_kind: &str,
) -> Result<AgentSourceSyncRunRow, GatewayError> {
    sqlx::query_as::<_, AgentSourceSyncRunRow>(
        r#"
        INSERT INTO "LiteLLM_AgentSourceSyncRunsTable" (
          id, source_id, connector_id, status, trigger_kind, started_at
        ) VALUES ($1, $2, $3, 'running', $4, $5)
        RETURNING *
        "#,
    )
    .bind(id("sync"))
    .bind(&source.id)
    .bind(&source.connector_id)
    .bind(trigger_kind)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn finish_sync_run(
    pool: &PgPool,
    run_id: &str,
    status: &str,
    changed_count: i32,
    missing_count: i32,
    error_detail: Option<&str>,
) -> Result<AgentSourceSyncRunRow, GatewayError> {
    sqlx::query_as::<_, AgentSourceSyncRunRow>(
        r#"
        UPDATE "LiteLLM_AgentSourceSyncRunsTable"
        SET status = $2, changed_count = $3, missing_count = $4,
            error_detail = $5, finished_at = $6
        WHERE id = $1 RETURNING *
        "#,
    )
    .bind(run_id)
    .bind(status)
    .bind(changed_count)
    .bind(missing_count)
    .bind(error_detail)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

/// Most recent sync attempt for a source, whatever its outcome — used to
/// surface the actual failure reason (not just the bare `sync_error` state
/// name) wherever the source's health is reported.
pub async fn latest_sync_run(
    pool: &PgPool,
    source_id: &str,
) -> Result<Option<AgentSourceSyncRunRow>, GatewayError> {
    sqlx::query_as::<_, AgentSourceSyncRunRow>(
        r#"
        SELECT * FROM "LiteLLM_AgentSourceSyncRunsTable"
        WHERE source_id = $1
        ORDER BY started_at DESC
        LIMIT 1
        "#,
    )
    .bind(source_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn mark_sync_state(
    pool: &PgPool,
    source_id: &str,
    sync_state: &str,
    missing_count: i32,
) -> Result<ManagedAgentSourceRow, GatewayError> {
    sqlx::query_as::<_, ManagedAgentSourceRow>(
        r#"
        UPDATE "LiteLLM_ManagedAgentSourcesTable"
        SET sync_state = $2, missing_count = $3, last_synced_at = $4,
            next_sync_at = $4 + 300000, lease_owner = NULL, lease_until = NULL,
            updated_at = $4
        WHERE id = $1 RETURNING *
        "#,
    )
    .bind(source_id)
    .bind(sync_state)
    .bind(missing_count)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

/// Most-recent statuses of one check kind for an agent, newest first. Lets
/// the auto-pause logic require N consecutive failures instead of reacting
/// to a single (possibly transient) one.
pub async fn recent_health_statuses(
    pool: &PgPool,
    agent_id: &str,
    check_kind: &str,
    limit: i64,
) -> Result<Vec<String>, GatewayError> {
    sqlx::query_scalar::<_, String>(
        r#"
        SELECT status FROM "LiteLLM_AgentHealthChecksTable"
        WHERE agent_id = $1 AND check_kind = $2
        ORDER BY checked_at DESC
        LIMIT $3
        "#,
    )
    .bind(agent_id)
    .bind(check_kind)
    .bind(limit.clamp(1, 20))
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn record_health(
    pool: &PgPool,
    agent_id: &str,
    source_id: Option<&str>,
    check_kind: &str,
    status: &str,
    detail: Option<&str>,
    latency_ms: Option<i64>,
) -> Result<AgentHealthCheckRow, GatewayError> {
    sqlx::query_as::<_, AgentHealthCheckRow>(
        r#"
        INSERT INTO "LiteLLM_AgentHealthChecksTable" (
          id, agent_id, source_id, check_kind, status, detail, latency_ms, checked_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING *
        "#,
    )
    .bind(id("health"))
    .bind(agent_id)
    .bind(source_id)
    .bind(check_kind)
    .bind(status)
    .bind(detail)
    .bind(latency_ms)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn upsert_conformance(
    pool: &PgPool,
    agent_id: &str,
    status: &str,
    checks: Value,
    revision: Option<i32>,
) -> Result<RuntimeConformanceRow, GatewayError> {
    sqlx::query_as::<_, RuntimeConformanceRow>(
        r#"
        INSERT INTO "LiteLLM_AgentRuntimeConformanceTable" (
          agent_id, contract_version, status, checks, checked_revision, checked_at
        ) VALUES ($1, 'lap-runtime-v1', $2, $3, $4, $5)
        ON CONFLICT (agent_id) DO UPDATE SET
          contract_version = EXCLUDED.contract_version,
          status = EXCLUDED.status, checks = EXCLUDED.checks,
          checked_revision = EXCLUDED.checked_revision,
          checked_at = EXCLUDED.checked_at
        RETURNING *
        "#,
    )
    .bind(agent_id)
    .bind(status)
    .bind(checks)
    .bind(revision)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn cancel_agent_work(pool: &PgPool, agent_id: &str) -> Result<u64, GatewayError> {
    let now = now_ms();
    let sessions = sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentSessionsTable"
        SET status = 'cancelled', updated_at = $2
        WHERE agent_id = $1
          AND COALESCE(status, 'idle') NOT IN ('cancelled', 'timed_out', 'completed', 'failed', 'error')
        "#,
    )
    .bind(agent_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?
    .rows_affected();
    let runs = sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentRunsTable"
        SET status = 'cancelled', finished_at = $2
        WHERE agent_id = $1
          AND status NOT IN ('cancelled', 'timed_out', 'completed', 'failed')
        "#,
    )
    .bind(agent_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?
    .rows_affected();
    // Converge every non-terminal turn of this agent's sessions: the session
    // sweep above changes only session status, and an orphaned running /
    // waiting_approval turn would otherwise stay "active" forever.
    sqlx::query(
        r#"
        UPDATE "LiteLLM_SessionTurnsTable"
        SET status = 'cancelled',
            error_json = '{"code": "agent_stopped", "message": "智能体已被紧急停止或退役。"}'::jsonb,
            completed_at = $2, updated_at = $2
        WHERE session_id IN (
            SELECT id FROM "LiteLLM_ManagedAgentSessionsTable" WHERE agent_id = $1
        )
          AND status NOT IN ('completed', 'failed', 'rejected', 'cancelled', 'timed_out')
        "#,
    )
    .bind(agent_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    sqlx::query(
        r#"
        UPDATE "LiteLLM_AgentSessionCapabilityTokensTable"
        SET revoked_at = $2 WHERE agent_id = $1 AND revoked_at IS NULL
        "#,
    )
    .bind(agent_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(sessions.saturating_add(runs))
}

pub async fn detach_source(pool: &PgPool, agent_id: &str) -> Result<bool, GatewayError> {
    let result = sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentSourcesTable"
        SET sync_state = 'detached', connector_id = NULL,
            lease_owner = NULL, lease_until = NULL, updated_at = $2
        WHERE agent_id = $1
        "#,
    )
    .bind(agent_id)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(result.rows_affected() == 1)
}

pub async fn list_due_sources(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<ManagedAgentSourceRow>, GatewayError> {
    // LEFT JOIN: directly imported sources have no connector but still know
    // their provider/endpoint/credential via governance — they must get
    // scheduled sync (drift detection) and the post-sync health checks too,
    // not only when someone clicks "sync" manually. Only an explicitly
    // disabled connector opts a source out; an unreachable one keeps being
    // retried at the normal cadence so it recovers without manual help.
    sqlx::query_as::<_, ManagedAgentSourceRow>(
        r#"
        SELECT source.*
        FROM "LiteLLM_ManagedAgentSourcesTable" source
        JOIN "LiteLLM_ManagedAgentsTable" agent ON agent.id = source.agent_id
        LEFT JOIN "LiteLLM_AgentSourceConnectorsTable" connector
          ON connector.id = source.connector_id
        WHERE source.sync_state != 'detached'
          AND agent.status != 'archived_pending_delete'
          AND NOT (agent.config ? 'deleted_at')
          AND (source.connector_id IS NULL OR connector.status IS DISTINCT FROM 'disabled')
          AND COALESCE(source.next_sync_at, 0) <= $1
        ORDER BY COALESCE(source.next_sync_at, 0) ASC
        LIMIT $2
        "#,
    )
    .bind(now_ms())
    .bind(limit.clamp(1, 100))
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn issue_capability_token(
    pool: &PgPool,
    session_id: &str,
    agent_id: &str,
    capabilities: Value,
    ttl_ms: i64,
) -> Result<(String, i64), GatewayError> {
    let now = now_ms();
    let expires_at = now.saturating_add(ttl_ms.clamp(60_000, 24 * 60 * 60 * 1000));
    let token = format!(
        "cap_{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let token_hash = format!("{:x}", Sha256::digest(token.as_bytes()));
    sqlx::query(
        r#"
        INSERT INTO "LiteLLM_AgentSessionCapabilityTokensTable" (
          session_id, agent_id, token_hash, capabilities, expires_at, created_at
        ) VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (session_id) DO UPDATE SET
          token_hash = EXCLUDED.token_hash, capabilities = EXCLUDED.capabilities,
          expires_at = EXCLUDED.expires_at, revoked_at = NULL,
          created_at = EXCLUDED.created_at
        "#,
    )
    .bind(session_id)
    .bind(agent_id)
    .bind(token_hash)
    .bind(capabilities)
    .bind(expires_at)
    .bind(now)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok((token, expires_at))
}

pub async fn validate_capability_token(
    pool: &PgPool,
    session_id: &str,
    token: &str,
) -> Result<bool, GatewayError> {
    let token_hash = format!("{:x}", Sha256::digest(token.as_bytes()));
    sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
          SELECT 1 FROM "LiteLLM_AgentSessionCapabilityTokensTable"
          WHERE session_id = $1 AND token_hash = $2
            AND revoked_at IS NULL AND expires_at > $3
        )
        "#,
    )
    .bind(session_id)
    .bind(token_hash)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn accept_webhook_delivery(
    pool: &PgPool,
    connector_id: &str,
    event_id: &str,
) -> Result<bool, GatewayError> {
    let inserted = sqlx::query(
        r#"
        INSERT INTO "LiteLLM_AgentConnectorWebhookDeliveriesTable" (
          connector_id, event_id, received_at
        ) VALUES ($1, $2, $3)
        ON CONFLICT (connector_id, event_id) DO NOTHING
        "#,
    )
    .bind(connector_id)
    .bind(event_id)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?
    .rows_affected()
        == 1;
    if inserted {
        sqlx::query(
            r#"
            UPDATE "LiteLLM_ManagedAgentSourcesTable"
            SET next_sync_at = 0, updated_at = $2 WHERE connector_id = $1
            "#,
        )
        .bind(connector_id)
        .bind(now_ms())
        .execute(pool)
        .await
        .map_err(GatewayError::Database)?;
    }
    Ok(inserted)
}
