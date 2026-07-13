use serde::Serialize;

use crate::sdk::agents::{CLAUDE_MANAGED_AGENTS, CURSOR, GEMINI_ANTIGRAVITY};

#[derive(Debug, Clone, Copy, Serialize)]
pub struct RuntimeTool {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub enabled_by_default: bool,
    /// Side-effect warning surfaced next to the tool checkbox; None for
    /// read-only tools with no external data flow.
    pub risk: Option<&'static str>,
}

pub fn runtime_tools(runtime: &str) -> &'static [RuntimeTool] {
    match runtime {
        CLAUDE_MANAGED_AGENTS => &CLAUDE_MANAGED_TOOLS,
        GEMINI_ANTIGRAVITY => &GEMINI_ANTIGRAVITY_TOOLS,
        CURSOR => &[],
        _ => &[],
    }
}

/// Whether write-approval is enforced by the platform at the tool-execution
/// boundary for this runtime, or merely suggested to the model via the
/// approval MCP. Native tools (bash/write/edit) execute inside the runtime
/// environment, outside LAP's dispatch path — with one exception: our own
/// opencode wrapper bridges opencode's native `Permission.Service.ask` gate
/// into LAP's inbox (see runtime_provision::provider_options and
/// managed_agents::tool_approvals), so opencode can genuinely block a tool
/// call pending human approval. Custom harnesses register under the same
/// api_spec as the built-in runtime they mimic (e.g. opencode speaks
/// claude_managed_agents), so `is_custom_harness` — not the api_spec string —
/// is what identifies "this is actually our wrapper".
pub fn approval_enforcement(runtime: &str, is_custom_harness: bool) -> &'static str {
    if is_custom_harness && runtime == CLAUDE_MANAGED_AGENTS {
        "enforced"
    } else {
        "advisory"
    }
}

const CLAUDE_MANAGED_TOOLS: [RuntimeTool; 8] = [
    RuntimeTool {
        id: "bash",
        name: "Shell",
        description: "Run shell commands in the agent environment.",
        enabled_by_default: false,
        risk: Some("可执行任意命令，包括删除数据、安装软件与调用外部服务"),
    },
    RuntimeTool {
        id: "read",
        name: "Read files",
        description: "Read files from the agent environment.",
        enabled_by_default: true,
        risk: None,
    },
    RuntimeTool {
        id: "write",
        name: "Write files",
        description: "Create or overwrite files in the agent environment.",
        enabled_by_default: false,
        risk: Some("可创建或覆盖运行环境中的任意文件"),
    },
    RuntimeTool {
        id: "edit",
        name: "Edit files",
        description: "Patch existing files in the agent environment.",
        enabled_by_default: false,
        risk: Some("可修改运行环境中的现有文件"),
    },
    RuntimeTool {
        id: "glob",
        name: "Find files",
        description: "Find files by glob pattern.",
        enabled_by_default: true,
        risk: None,
    },
    RuntimeTool {
        id: "grep",
        name: "Search files",
        description: "Search file contents by regular expression.",
        enabled_by_default: true,
        risk: None,
    },
    RuntimeTool {
        id: "web_fetch",
        name: "Fetch URL",
        description: "Fetch content from a URL.",
        enabled_by_default: false,
        risk: Some("会向外部站点发起请求，URL 与参数可能携带敏感数据外传"),
    },
    RuntimeTool {
        id: "web_search",
        name: "Web search",
        description: "Search the web for information.",
        enabled_by_default: true,
        risk: Some("搜索词会发送到外部搜索引擎"),
    },
];

const GEMINI_ANTIGRAVITY_TOOLS: [RuntimeTool; 3] = [
    RuntimeTool {
        id: "code_execution",
        name: "Code execution",
        description: "Run code and shell commands in the managed sandbox.",
        enabled_by_default: false,
        risk: Some("可在托管沙箱中运行任意代码与命令"),
    },
    RuntimeTool {
        id: "google_search",
        name: "Google Search",
        description: "Search the public web.",
        enabled_by_default: true,
        risk: Some("搜索词会发送到外部搜索引擎"),
    },
    RuntimeTool {
        id: "url_context",
        name: "URL context",
        description: "Fetch and read web pages.",
        enabled_by_default: true,
        risk: Some("会向外部站点发起请求"),
    },
];
