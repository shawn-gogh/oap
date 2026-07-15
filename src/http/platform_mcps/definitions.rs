use serde_json::{json, Value};

use super::{
    factory, session_management, AGENT_MEMORY_MCP_ID, CHECK_HUMAN_APPROVAL_MCP_ID,
    EDIT_AGENT_SKILL_MCP_ID, EXPOSE_PORT_MCP_ID, LIST_SUB_AGENTS_MCP_ID,
    REQUEST_HUMAN_APPROVAL_MCP_ID, RUN_SUB_AGENT_MCP_ID,
};

pub fn tool_defs() -> Vec<Value> {
    let mut tools = vec![
        session_management::read_tool_def(),
        session_management::send_tool_def(),
        agent_memory_tool(),
        edit_agent_skill_tool(),
        list_sub_agents_tool(),
        run_sub_agent_tool(),
        request_human_approval_tool(),
        check_human_approval_tool(),
        expose_port_tool(),
    ];
    tools.extend(factory::tool_defs());
    tools
}

fn request_human_approval_tool() -> Value {
    json!({
        "name": REQUEST_HUMAN_APPROVAL_MCP_ID,
        "description": "Create an async human approval request in the Agent Inbox. This call returns immediately with a pending approval_id; continue other safe work or call check_human_approval later.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Short approval title shown in the inbox."
                },
                "body": {
                    "type": "string",
                    "description": "Context, risk, and exact action the human should review."
                },
                "session_id": {
                    "type": "string",
                    "description": "Optional LAP session ID so the inbox item can link back to the conversation."
                },
                "arguments": {
                    "type": "object",
                    "description": "Optional structured action arguments the human may edit before approving."
                },
                "options": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "description": "Optional list of predefined choices or suggested answers/feedback for the user."
                }
            },
            "required": ["title"]
        }
    })
}

fn expose_port_tool() -> Value {
    json!({
        "name": EXPOSE_PORT_MCP_ID,
        "description": "Expose an interactive service (dashboard, live UI, WebSocket app) to the user's browser. Call this BEFORE starting the server: the platform allocates a port, registers it, and returns a public URL. You must then start your HTTP/WebSocket server listening on 0.0.0.0 at the returned port. Serve the app with root-relative or relative asset paths.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "port": {
                    "type": "integer",
                    "description": "Optional specific port (1024-65535). Omit to let the platform allocate one from its managed range (recommended)."
                },
                "name": {
                    "type": "string",
                    "description": "Optional human-readable name for this app (e.g. 'sales-dashboard')."
                },
                "ttl_seconds": {
                    "type": "integer",
                    "description": "Optional lifetime in seconds before the exposure is revoked. Defaults to 24 hours."
                }
            }
        }
    })
}

fn check_human_approval_tool() -> Value {
    json!({
        "name": CHECK_HUMAN_APPROVAL_MCP_ID,
        "description": "Check whether a previously filed human approval request is pending, accepted, rejected, or missing.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "approval_id": {
                    "type": "string",
                    "description": "Approval ID returned by request_human_approval."
                }
            },
            "required": ["approval_id"]
        }
    })
}

fn list_sub_agents_tool() -> Value {
    json!({
        "name": LIST_SUB_AGENTS_MCP_ID,
        "description": "List this parent agent's attached LAP sub-agents, including each agent_id, name, description, model, and runtime. Call this before run_sub_agent when choosing by name.",
        "inputSchema": {
            "type": "object",
            "properties": {}
        }
    })
}

fn run_sub_agent_tool() -> Value {
    json!({
        "name": RUN_SUB_AGENT_MCP_ID,
        "description": "Run one of this agent's configured LAP sub-agents. Only agent IDs attached to this parent agent are allowed. Use list_sub_agents first when you need the attached agents' names.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "The LAP agent ID from this parent agent's sub_agents list."
                },
                "prompt": {
                    "type": "string",
                    "description": "The complete task, context, paths, and expected output for the sub-agent."
                },
                "title": {
                    "type": "string",
                    "description": "Optional session title."
                }
            },
            "required": ["agent_id", "prompt"]
        }
    })
}

fn agent_memory_tool() -> Value {
    json!({
        "name": AGENT_MEMORY_MCP_ID,
        "description": "List, read, or update DB-backed memory for this platform agent.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["list", "get", "set"] },
                "key": { "type": "string" },
                "value": { "type": "string" },
                "always_on": { "type": "boolean" }
            },
            "required": ["action"]
        }
    })
}

fn edit_agent_skill_tool() -> Value {
    json!({
        "name": EDIT_AGENT_SKILL_MCP_ID,
        "description": "List, read, or update DB-backed skills attached to this agent. Updates are limited to this agent's own attached skill_ids.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["list", "get", "update"] },
                "skill_id": {
                    "type": "string",
                    "description": "Attached skill ID. May be omitted for get/update only when exactly one skill is attached."
                },
                "name": { "type": "string" },
                "description": { "type": "string" },
                "content": {
                    "type": "string",
                    "description": "Full replacement markdown content for the skill."
                }
            },
            "required": ["action"]
        }
    })
}
