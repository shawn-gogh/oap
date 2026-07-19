use sqlx::PgPool;
use tracing::warn;

use crate::{
    db::managed_agents::registry::schema::ManagedAgentRow, errors::GatewayError,
    proxy::state::AppState,
};

use super::{
    config::{bot_token_key, load_secret, mattermost_config},
    web_api,
};

pub(crate) enum GovernanceNotification<'a> {
    PublishRequested {
        approval_id: &'a str,
        base_revision: i32,
        revision: i32,
    },
    HealthDegraded {
        consecutive_failures: i64,
        detail: &'a str,
    },
    HighRiskDrift {
        snapshot_id: &'a str,
        highest_risk: &'a str,
        changed_fields: &'a [String],
    },
    ReviewDue {
        review_due_at: i64,
    },
}

pub(crate) async fn notify_governance_event(
    state: &AppState,
    pool: &PgPool,
    agent: &ManagedAgentRow,
    event: GovernanceNotification<'_>,
) {
    if let Err(error) = deliver_governance_event(state, pool, agent, event).await {
        warn!(agent_id = %agent.id, %error, "mattermost governance notification failed");
    }
}

async fn deliver_governance_event(
    state: &AppState,
    _pool: &PgPool,
    agent: &ManagedAgentRow,
    event: GovernanceNotification<'_>,
) -> Result<(), GatewayError> {
    let config = mattermost_config(agent)?;
    if config.status.as_deref() != Some("connected") {
        return Ok(());
    }
    let Some(server_url) = non_empty(config.server_url.as_deref()) else {
        return Ok(());
    };
    let Some(channel_id) = non_empty(config.notification_channel_id.as_deref()) else {
        return Ok(());
    };
    let bot_token = load_secret(state, &bot_token_key(&agent.id, &config)).await?;
    let public_base_url = state
        .config
        .general_settings
        .public_base_url
        .clone()
        .or_else(|| state.resolved_mcp_proxy_base_url());
    let text = notification_text(public_base_url.as_deref(), &agent.id, &agent.name, event);
    web_api::create_channel_post(&state.http, server_url, &bot_token, channel_id, &text).await?;
    Ok(())
}

fn notification_text(
    public_base_url: Option<&str>,
    agent_id: &str,
    agent_name: &str,
    event: GovernanceNotification<'_>,
) -> String {
    let agent_link = link(
        public_base_url,
        &format!("/agents/detail/?id={agent_id}"),
        "查看智能体",
    );
    // The agent name is owner-controlled free text that lands in a shared
    // operator channel; neutralize Markdown / @mentions before interpolating.
    let agent_name = sanitize_markdown(agent_name);
    let agent_name = agent_name.as_str();
    match event {
        GovernanceNotification::PublishRequested {
            approval_id,
            base_revision,
            revision,
        } => {
            let approval_link = link(public_base_url, "/inbox/", "前往审批");
            format!(
                "### 待审批：智能体发布\n**{}** 申请从 v{} 发布到 v{}。\n审批 ID：`{}`\n{} · {}",
                agent_name,
                base_revision,
                revision,
                approval_id,
                approval_link,
                agent_link
            )
        }
        GovernanceNotification::HealthDegraded {
            consecutive_failures,
            detail,
        } => format!(
            "### 健康告警：智能体已自动暂停\n**{}** 连续 {} 次健康检查未通过，新工作已暂停。\n{}\n{}",
            agent_name, consecutive_failures, detail, agent_link
        ),
        GovernanceNotification::HighRiskDrift {
            snapshot_id,
            highest_risk,
            changed_fields,
        } => {
            let fields = changed_fields
                .iter()
                .take(8)
                .map(|field| format!("`{field}`"))
                .collect::<Vec<_>>()
                .join("、");
            format!(
                "### 高风险漂移：智能体已自动暂停\n**{}** 检测到 {} 风险来源变更，新工作已暂停。\n变更字段：{}\n快照 ID：`{}`\n{}",
                agent_name, highest_risk, fields, snapshot_id, agent_link
            )
        }
        GovernanceNotification::ReviewDue { review_due_at } => {
            review_due_text(public_base_url, agent_name, review_due_at, &agent_link)
        }
    }
}

fn review_due_text(
    public_base_url: Option<&str>,
    agent_name: &str,
    review_due_at: i64,
    agent_link: &str,
) -> String {
    let approval_link = link(public_base_url, "/inbox/", "复审通过后前往审批");
    format!(
        "### 定期复审：智能体发布已到期\n**{}** 的发布有效期已于 {} 到期，新工作已暂停。\n请重新运行治理检查并申请发布复审。\n{} · {}",
        agent_name,
        format_timestamp(review_due_at),
        approval_link,
        agent_link
    )
}

/// Neutralizes owner-controlled text before it is interpolated into the
/// Markdown notification body posted to a shared operator channel. Collapses
/// whitespace to a single line, backslash-escapes Markdown punctuation, and
/// breaks `@` mention tokens with a zero-width space so a crafted agent name
/// can't inject formatting or ping the whole channel (`@channel`/`@here`).
fn sanitize_markdown(value: &str) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = String::with_capacity(collapsed.len() + 8);
    for ch in collapsed.chars() {
        match ch {
            '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '#' | '+' | '-' | '.'
            | '!' | '|' | '>' | '~' => {
                out.push('\\');
                out.push(ch);
            }
            '@' => {
                out.push('@');
                out.push('\u{200b}');
            }
            _ => out.push(ch),
        }
    }
    out
}

fn format_timestamp(timestamp_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(timestamp_ms)
        .map(|value| value.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|| timestamp_ms.to_string())
}

fn link(public_base_url: Option<&str>, path: &str, label: &str) -> String {
    match public_base_url {
        Some(base) => format!("[{label}]({}{path})", base.trim_end_matches('/')),
        None => label.to_owned(),
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{notification_text, GovernanceNotification};

    #[test]
    fn publish_notification_contains_revisions_and_direct_links() {
        let text = notification_text(
            Some("https://lap.example.com/"),
            "agent-1",
            "发布助手",
            GovernanceNotification::PublishRequested {
                approval_id: "approval-1",
                base_revision: 2,
                revision: 3,
            },
        );

        assert!(text.contains("从 v2 发布到 v3"));
        assert!(text.contains("`approval-1`"));
        assert!(text.contains("[前往审批](https://lap.example.com/inbox/)"));
        assert!(text.contains("[查看智能体](https://lap.example.com/agents/detail/?id=agent-1)"));
    }

    #[test]
    fn health_notification_contains_pause_reason() {
        let text = notification_text(
            None,
            "agent-1",
            "巡检助手",
            GovernanceNotification::HealthDegraded {
                consecutive_failures: 3,
                detail: "MCP 连接失败。",
            },
        );

        assert!(text.contains("连续 3 次健康检查未通过"));
        assert!(text.contains("MCP 连接失败。"));
        assert!(text.contains("查看智能体"));
        assert!(!text.contains("]("));
    }

    #[test]
    fn drift_notification_lists_only_supplied_risk_fields() {
        let fields = vec!["tools".to_owned(), "config.runtime".to_owned()];
        let text = notification_text(
            Some("https://lap.example.com"),
            "agent-1",
            "同步助手",
            GovernanceNotification::HighRiskDrift {
                snapshot_id: "snapshot-1",
                highest_risk: "critical",
                changed_fields: &fields,
            },
        );

        assert!(text.contains("critical 风险"));
        assert!(text.contains("`tools`、`config.runtime`"));
        assert!(text.contains("`snapshot-1`"));
    }

    #[test]
    fn agent_name_markdown_and_mentions_are_neutralized() {
        let text = notification_text(
            None,
            "agent-1",
            "@channel **紧急**",
            GovernanceNotification::HealthDegraded {
                consecutive_failures: 1,
                detail: "x",
            },
        );

        // The raw mention token and bold markers must not survive verbatim.
        assert!(!text.contains("@channel"));
        assert!(!text.contains("**紧急**"));
        // The @ still renders but is broken by a zero-width space, and the
        // asterisks are backslash-escaped.
        assert!(text.contains("@\u{200b}channel"));
        assert!(text.contains("\\*\\*紧急\\*\\*"));
    }

    #[test]
    fn review_notification_explains_pause_and_next_steps() {
        let text = notification_text(
            Some("https://lap.example.com"),
            "agent-1",
            "复审助手",
            GovernanceNotification::ReviewDue {
                review_due_at: 1_767_225_600_000,
            },
        );

        assert!(text.contains("发布有效期已于 2026-01-01 00:00 UTC 到期"));
        assert!(text.contains("新工作已暂停"));
        assert!(text.contains("重新运行治理检查"));
    }
}
