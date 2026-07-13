use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct AgentTaskRow {
    pub id: String,
    pub agent_id: String,
    pub application_version: i32,
    pub source: String,
    pub source_id: Option<String>,
    pub title: String,
    pub input_json: Value,
    pub status: String,
    pub created_by: String,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub failure_reason: Option<String>,
    pub failure_code: Option<String>,
    pub deadline_at: Option<i64>,
    pub current_attempt_number: i32,
}

#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub title: Option<String>,
    pub input: Option<Value>,
    pub source: Option<String>,
}

pub struct NewTask<'a> {
    pub agent_id: &'a str,
    pub application_version: i32,
    pub source: &'a str,
    pub source_id: Option<&'a str>,
    pub title: &'a str,
    pub input: Value,
    pub created_by: &'a str,
    pub completion_criteria: Vec<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct TaskArtifactRow {
    pub id: String,
    pub task_id: String,
    pub session_id: Option<String>,
    pub run_id: Option<String>,
    pub attempt_number: i32,
    pub artifact_type: String,
    pub name: String,
    pub content_json: Option<Value>,
    pub location: Option<String>,
    pub dedupe_key: Option<String>,
    pub created_by: String,
    pub created_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateArtifactRequest {
    pub artifact_type: String,
    pub name: String,
    pub content: Option<Value>,
    pub location: Option<String>,
    pub session_id: Option<String>,
}

pub struct NewArtifact<'a> {
    pub task_id: &'a str,
    pub session_id: Option<&'a str>,
    pub run_id: Option<&'a str>,
    pub artifact_type: &'a str,
    pub name: &'a str,
    pub content: Option<Value>,
    pub location: Option<&'a str>,
    pub dedupe_key: Option<&'a str>,
    pub created_by: &'a str,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct TaskAcceptanceCheckRow {
    pub id: String,
    pub task_id: String,
    pub attempt_number: i32,
    pub criterion_index: i32,
    pub criterion: String,
    pub verdict: String,
    pub evidence: Option<String>,
    pub checked_by: Option<String>,
    pub checked_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAcceptanceRequest {
    pub criterion_index: i32,
    pub verdict: String,
    pub evidence: Option<String>,
    pub criterion: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ResumeTaskRequest {
    pub input: Value,
}

#[derive(Debug, Deserialize)]
pub struct RetryTaskRequest {
    pub runtime: Option<String>,
}

pub struct TaskCancellation {
    pub task: AgentTaskRow,
    pub session_id: Option<String>,
    pub run_id: Option<String>,
}
