use serde::Serialize;

use super::{
    AGENT_MEMORY_MCP_ID, CHECK_HUMAN_APPROVAL_MCP_ID, CREATE_MANAGED_AGENT_MCP_ID,
    EDIT_AGENT_SKILL_MCP_ID, EXPOSE_PORT_MCP_ID, LIST_SUB_AGENTS_MCP_ID, PLATFORM_SESSION_MCP_ID,
    REQUEST_HUMAN_APPROVAL_MCP_ID, RUN_SUB_AGENT_MCP_ID, SEND_PLATFORM_SESSION_MESSAGE_MCP_ID,
};

#[derive(Debug, Clone, Copy, Serialize)]
pub struct PlatformMcp {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
}

pub fn platform_mcps() -> Vec<PlatformMcp> {
    CATALOG.to_vec()
}

const CATALOG: &[PlatformMcp] = &[
    PlatformMcp {
        id: PLATFORM_SESSION_MCP_ID,
        name: "读取平台会话",
        description: "读取已持久化的平台会话消息，用于调试与交接。",
    },
    PlatformMcp {
        id: SEND_PLATFORM_SESSION_MESSAGE_MCP_ID,
        name: "发送平台会话消息",
        description: "向某个平台会话发送一条用户消息，并恢复该智能体的运行。",
    },
    PlatformMcp {
        id: AGENT_MEMORY_MCP_ID,
        name: "读写智能体记忆",
        description: "列出、读取并更新平台智能体存储于数据库的记忆。",
    },
    PlatformMcp {
        id: EDIT_AGENT_SKILL_MCP_ID,
        name: "编辑智能体技能",
        description: "列出、读取并更新附加到该智能体、存储于数据库的技能。",
    },
    PlatformMcp {
        id: CREATE_MANAGED_AGENT_MCP_ID,
        name: "创建托管智能体",
        description: "根据一次对话请求创建一个托管智能体。",
    },
    PlatformMcp {
        id: LIST_SUB_AGENTS_MCP_ID,
        name: "列出子智能体",
        description: "列出该智能体附加的 LAP 子智能体，包含 ID、名称与运行时。",
    },
    PlatformMcp {
        id: RUN_SUB_AGENT_MCP_ID,
        name: "运行子智能体",
        description:
            "运行该智能体显式附加的某个 LAP 子智能体，并返回其会话。",
    },
    PlatformMcp {
        id: REQUEST_HUMAN_APPROVAL_MCP_ID,
        name: "请求人工审批",
        description: "在托管智能体收件箱中提交一条异步的操作员审批请求。",
    },
    PlatformMcp {
        id: CHECK_HUMAN_APPROVAL_MCP_ID,
        name: "查询人工审批",
        description: "查询已提交审批请求的当前决策状态。",
    },
    PlatformMcp {
        id: EXPOSE_PORT_MCP_ID,
        name: "暴露服务端口",
        description: "注册一个容器端口，使智能体在该端口上启动的服务可经由网关从宿主机浏览器访问。",
    },
];
