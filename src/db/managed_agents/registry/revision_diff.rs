use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::Value;

const REVIEWED_FIELDS: &[&str] = &[
    "name",
    "description",
    "model",
    "system",
    "tools",
    "cron",
    "timezone",
    "vault_keys",
    "setup_commands",
    "max_runtime_minutes",
    "on_failure",
    "config",
    "harness",
    "skill_ids",
    "rule_ids",
];

#[derive(Debug, Clone, Serialize)]
pub struct RevisionDiffFinding {
    pub field_path: String,
    pub risk: &'static str,
    pub change_type: &'static str,
    pub previous_value: Option<Value>,
    pub candidate_value: Option<Value>,
}

pub fn compare(previous: &Value, candidate: &Value) -> Vec<RevisionDiffFinding> {
    let mut findings = Vec::new();
    for field in REVIEWED_FIELDS {
        collect(
            field,
            previous.get(field),
            candidate.get(field),
            &mut findings,
        );
    }
    findings
}

pub fn highest_risk(findings: &[RevisionDiffFinding]) -> &'static str {
    findings
        .iter()
        .map(|finding| finding.risk)
        .max_by_key(|risk| risk_rank(risk))
        .unwrap_or("low")
}

fn collect(
    path: &str,
    previous: Option<&Value>,
    candidate: Option<&Value>,
    findings: &mut Vec<RevisionDiffFinding>,
) {
    if previous == candidate {
        return;
    }
    if let (Some(Value::Object(before)), Some(Value::Object(after))) = (previous, candidate) {
        let keys: BTreeSet<_> = before.keys().chain(after.keys()).collect();
        for key in keys {
            collect(
                &format!("{path}.{key}"),
                before.get(key),
                after.get(key),
                findings,
            );
        }
        return;
    }
    findings.push(RevisionDiffFinding {
        field_path: path.to_owned(),
        risk: risk_for_path(path),
        change_type: change_type(previous, candidate),
        previous_value: previous.cloned(),
        candidate_value: candidate.cloned(),
    });
}

fn change_type(previous: Option<&Value>, candidate: Option<&Value>) -> &'static str {
    match (
        previous.filter(|value| !value.is_null()),
        candidate.filter(|value| !value.is_null()),
    ) {
        (None, Some(_)) => "added",
        (Some(_), None) => "removed",
        _ => "changed",
    }
}

fn risk_for_path(path: &str) -> &'static str {
    if path == "harness"
        || path == "vault_keys"
        || path.starts_with("config.runtime")
        || path.contains("mcp_server")
        || path.contains("network_access")
    {
        "critical"
    } else if path == "tools"
        || path == "setup_commands"
        || path == "cron"
        || path == "on_failure"
        || path == "skill_ids"
        || path == "rule_ids"
        || path.contains("filesystem_access")
        || path.contains("side_effect")
    {
        "high"
    } else if path == "model" || path == "system" || path == "max_runtime_minutes" {
        "medium"
    } else {
        "low"
    }
}

fn risk_rank(risk: &str) -> u8 {
    match risk {
        "critical" => 4,
        "high" => 3,
        "medium" => 2,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn excludes_runtime_identity_and_classifies_sensitive_changes() {
        let previous = json!({
            "id": "agent-old",
            "created_at": 1,
            "model": "model-a",
            "config": {"runtime": "opencode", "theme": "blue"},
            "tools": ["read"]
        });
        let candidate = json!({
            "id": "agent-new",
            "created_at": 2,
            "model": "model-b",
            "config": {"runtime": "cursor", "theme": "blue"},
            "tools": ["read", "bash"]
        });
        let findings = compare(&previous, &candidate);
        assert_eq!(findings.len(), 3);
        assert!(findings
            .iter()
            .any(|finding| finding.field_path == "config.runtime" && finding.risk == "critical"));
        assert_eq!(highest_risk(&findings), "critical");
    }
}
