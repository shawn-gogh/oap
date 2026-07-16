//! Guardian reviewer — an independent LLM call that reviews a risky action
//! and returns a structured allow/deny verdict, instead of a static rule
//! auto-deciding. Modeled on `openai/codex`'s `codex-rs/core/src/guardian`
//! (verified against the real source, not from memory): same field names
//! (`risk_level`/`user_authorization`/`outcome`/`rationale`), same
//! circuit-breaker thresholds (3 consecutive denials, or 10 of the last 50),
//! same fail-closed contract on parse/timeout/call failure.
//!
//! This sits behind the existing `approval_mode == "auto"` ("替我审批")
//! session setting — `full` bypasses review entirely (Codex's `never`),
//! `ask` never reaches here (Codex's `untrusted`, always human).

mod prompt;

use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
    time::Duration,
};

use serde::Deserialize;
use sqlx::PgPool;

use crate::{
    db::managed_agents::settings, errors::GatewayError, http::managed_agents::eval_runs,
    proxy::state::AppState,
};

/// Codex's own default: generous because their review sits inside a
/// tolerant agent loop. Ours also gates a live TCP CONNECT in one call site
/// (`egress_proxy`), where the client's own timeout may be much shorter, so
/// the default here is intentionally tighter; still overridable.
fn review_timeout() -> Duration {
    let ms = std::env::var("GUARDIAN_REVIEW_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(25_000);
    Duration::from_millis(ms)
}

const MAX_CONSECUTIVE_DENIALS_PER_TURN: u32 = 3;
const MAX_RECENT_DENIALS_PER_TURN: u32 = 10;
const DENIAL_WINDOW_SIZE: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserAuthorization {
    Unknown,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Allow,
    Deny,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GuardianAssessment {
    pub risk_level: RiskLevel,
    pub user_authorization: UserAuthorization,
    pub outcome: Outcome,
    pub rationale: String,
}

/// Everything a call site has on hand to describe the action under review.
/// Both `tool_approvals::asked()` and `egress_proxy::decide()` build one of
/// these from context they already resolve for their own purposes.
pub struct GuardianContext {
    pub action_description: String,
    pub target: Option<String>,
    pub agent_name: Option<String>,
    pub recent_user_message: Option<String>,
    /// Model to use when no `guardian_model` override is configured —
    /// callers pass the session's own agent model, same fallback
    /// `eval_runs.rs`/`improvements.rs` already use for their internal calls.
    pub fallback_model: String,
}

pub struct GuardianVerdict {
    pub allow: bool,
    /// `None` when the call itself failed/timed out/didn't parse — the
    /// verdict is still fail-closed (`allow: false`) but there's no genuine
    /// assessment to show the human, only the failure reason in `reason`.
    pub assessment: Option<GuardianAssessment>,
    pub reason: String,
}

/// Runs one review. Always fail-closed: any error, timeout, or malformed
/// output resolves to `allow: false` — the caller decides what "denied"
/// means for its own choke point (route to a human, or hard-reject).
pub async fn review(state: &AppState, pool: &PgPool, context: &GuardianContext) -> GuardianVerdict {
    let model = settings::repository::get_guardian_model(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| context.fallback_model.clone());

    let user_message = prompt::build_user_message(context);
    let call = eval_runs::complete_text(
        state,
        &model,
        prompt::GUARDIAN_SYSTEM_PROMPT,
        &user_message,
        300,
    );

    let raw = match tokio::time::timeout(review_timeout(), call).await {
        Ok(Ok(text)) => text,
        Ok(Err(error)) => {
            return GuardianVerdict {
                allow: false,
                assessment: None,
                reason: format!("guardian call failed: {error}"),
            }
        }
        Err(_) => {
            return GuardianVerdict {
                allow: false,
                assessment: None,
                reason: "guardian review timed out".to_owned(),
            }
        }
    };

    match parse_assessment(&raw) {
        Ok(assessment) => {
            let allow = assessment.outcome == Outcome::Allow;
            let reason = assessment.rationale.clone();
            GuardianVerdict {
                allow,
                assessment: Some(assessment),
                reason,
            }
        }
        Err(error) => GuardianVerdict {
            allow: false,
            assessment: None,
            reason: format!("guardian returned unparseable output: {error}"),
        },
    }
}

fn parse_assessment(raw: &str) -> Result<GuardianAssessment, GatewayError> {
    let cleaned = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    serde_json::from_str(cleaned).map_err(|error| {
        GatewayError::SandboxError(format!("guardian JSON parse failed: {error}"))
    })
}

/// Per-session denial tracking, mirroring Codex's
/// `GuardianRejectionCircuitBreaker` exactly (same thresholds, same
/// latch-once-triggered behavior so a tripped breaker doesn't keep firing on
/// every subsequent denial in the same turn).
#[derive(Debug, Default)]
pub struct CircuitBreaker {
    turns: Mutex<HashMap<String, CircuitBreakerTurn>>,
}

#[derive(Debug, Default)]
struct CircuitBreakerTurn {
    consecutive_denials: u32,
    recent_denials: VecDeque<bool>,
    interrupt_triggered: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitBreakerAction {
    Continue,
    InterruptTurn {
        consecutive_denials: u32,
        recent_denials: u32,
    },
}

impl CircuitBreaker {
    pub fn clear_turn(&self, session_id: &str) {
        self.turns
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(session_id);
    }

    pub fn record_denial(&self, session_id: &str) -> CircuitBreakerAction {
        let mut turns = self
            .turns
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let turn = turns.entry(session_id.to_owned()).or_default();
        turn.consecutive_denials = turn.consecutive_denials.saturating_add(1);
        record_recent(turn, true);
        let recent_denials = turn.recent_denials.iter().filter(|denied| **denied).count() as u32;
        if !turn.interrupt_triggered
            && (turn.consecutive_denials >= MAX_CONSECUTIVE_DENIALS_PER_TURN
                || recent_denials >= MAX_RECENT_DENIALS_PER_TURN)
        {
            turn.interrupt_triggered = true;
            CircuitBreakerAction::InterruptTurn {
                consecutive_denials: turn.consecutive_denials,
                recent_denials,
            }
        } else {
            CircuitBreakerAction::Continue
        }
    }

    pub fn record_non_denial(&self, session_id: &str) {
        let mut turns = self
            .turns
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let turn = turns.entry(session_id.to_owned()).or_default();
        turn.consecutive_denials = 0;
        record_recent(turn, false);
    }
}

fn record_recent(turn: &mut CircuitBreakerTurn, denied: bool) {
    turn.recent_denials.push_back(denied);
    if turn.recent_denials.len() > DENIAL_WINDOW_SIZE {
        turn.recent_denials.pop_front();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trips_after_three_consecutive_denials() {
        let breaker = CircuitBreaker::default();
        assert_eq!(breaker.record_denial("s1"), CircuitBreakerAction::Continue);
        assert_eq!(breaker.record_denial("s1"), CircuitBreakerAction::Continue);
        assert_eq!(
            breaker.record_denial("s1"),
            CircuitBreakerAction::InterruptTurn {
                consecutive_denials: 3,
                recent_denials: 3,
            }
        );
    }

    #[test]
    fn does_not_retrigger_after_latching() {
        let breaker = CircuitBreaker::default();
        for _ in 0..3 {
            breaker.record_denial("s1");
        }
        assert_eq!(breaker.record_denial("s1"), CircuitBreakerAction::Continue);
    }

    #[test]
    fn non_denial_resets_consecutive_count() {
        let breaker = CircuitBreaker::default();
        breaker.record_denial("s1");
        breaker.record_denial("s1");
        breaker.record_non_denial("s1");
        assert_eq!(breaker.record_denial("s1"), CircuitBreakerAction::Continue);
    }

    #[test]
    fn trips_on_ten_of_last_fifty_without_three_consecutive() {
        let breaker = CircuitBreaker::default();
        // Alternate deny/allow nine times (5 denials, 4 non-denials) — no run
        // of 3 consecutive, but building toward the rolling-window threshold.
        for i in 0..9 {
            if i % 2 == 0 {
                assert_eq!(breaker.record_denial("s1"), CircuitBreakerAction::Continue);
            } else {
                breaker.record_non_denial("s1");
            }
        }
        // 5 denials so far; 5 more (still alternating, so never 3 in a row)
        // reaches 10 total within the 50-window and should trip.
        assert_eq!(breaker.record_denial("s1"), CircuitBreakerAction::Continue); // 6
        breaker.record_non_denial("s1");
        assert_eq!(breaker.record_denial("s1"), CircuitBreakerAction::Continue); // 7
        breaker.record_non_denial("s1");
        assert_eq!(breaker.record_denial("s1"), CircuitBreakerAction::Continue); // 8
        breaker.record_non_denial("s1");
        assert_eq!(breaker.record_denial("s1"), CircuitBreakerAction::Continue); // 9
        breaker.record_non_denial("s1");
        assert_eq!(
            breaker.record_denial("s1"), // 10
            CircuitBreakerAction::InterruptTurn {
                consecutive_denials: 1,
                recent_denials: 10,
            }
        );
    }

    #[test]
    fn clear_turn_resets_state() {
        let breaker = CircuitBreaker::default();
        breaker.record_denial("s1");
        breaker.record_denial("s1");
        breaker.clear_turn("s1");
        breaker.record_denial("s1");
        assert_eq!(breaker.record_denial("s1"), CircuitBreakerAction::Continue);
    }

    #[test]
    fn parses_valid_assessment_json() {
        let raw = r#"{"risk_level":"low","user_authorization":"high","outcome":"allow","rationale":"routine read"}"#;
        let assessment = parse_assessment(raw).expect("should parse");
        assert_eq!(assessment.outcome, Outcome::Allow);
        assert_eq!(assessment.risk_level, RiskLevel::Low);
    }

    #[test]
    fn parses_assessment_wrapped_in_markdown_fence() {
        let raw = "```json\n{\"risk_level\":\"high\",\"user_authorization\":\"unknown\",\"outcome\":\"deny\",\"rationale\":\"exfil risk\"}\n```";
        let assessment = parse_assessment(raw).expect("should parse");
        assert_eq!(assessment.outcome, Outcome::Deny);
    }

    #[test]
    fn rejects_missing_field() {
        let raw = r#"{"risk_level":"low","outcome":"allow","rationale":"x"}"#;
        assert!(parse_assessment(raw).is_err());
    }

    #[test]
    fn rejects_invalid_enum_value() {
        let raw = r#"{"risk_level":"extreme","user_authorization":"high","outcome":"allow","rationale":"x"}"#;
        assert!(parse_assessment(raw).is_err());
    }

    #[test]
    fn rejects_non_json_output() {
        assert!(parse_assessment("sure, I'll allow that").is_err());
    }
}
