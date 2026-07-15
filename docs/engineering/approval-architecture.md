# Approval architecture

Runtime-native permissions remain enforced by the runtime. LAP provides the
decision queue, role routing, delivery, retry, expiry, and audit trail, but a
runtime tool is not released until the runtime accepts LAP's reply.

| Kind | Enforcement owner | Effect handler | Initial role | TTL | Escalation |
|---|---|---|---|---:|---|
| `runtime_permission` | Runtime | Runtime permission reply | Session or agent owner | 5 minutes | None; expires deny |
| `data_egress` | Runtime | Runtime permission reply | Administrator | 15 minutes | None; expires deny |
| `business_decision` | Workflow | Resume linked session | Session or agent owner | 24 hours | Group administrator |
| `agent_publish` | Platform | Publish tested revision | Administrator | 7 days | None |
| `agent_change` | Platform | Apply approved revision | Agent owner | 7 days | Administrator |
| `platform_action` | Platform | Action-specific handler | Group administrator | 24 hours | Administrator |

Legacy `approval`, `tool_permission`, and `unlisted_data_egress` rows remain
readable and decidable. New producers must use a typed kind.

Approval decision state and effect delivery state are separate. A decision can
be `accepted`, `rejected`, or `expired` while delivery is `delivering`,
`applied`, or `delivery_failed`. Failed delivery stays in the attention queue
and can be retried. Runtime and governance request arguments are immutable;
only business-decision arguments may be edited by an approver.

The expiry sweeper runs independently of the originating process. Runtime
permissions therefore survive LAP restarts and fail closed when their TTL
elapses. Every decision, retry, escalation, and expiry is written to the audit
log.
