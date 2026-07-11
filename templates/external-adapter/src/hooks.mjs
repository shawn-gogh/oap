// ─────────────────────────────────────────────────────────────────────────────
// 接入方唯一需要修改的文件：把这 4 个钩子对接到你自己的智能体系统。
// 默认实现是一个 echo 智能体，让脚手架开箱即可注册、跑通全链路。
//
// The only file an integrator edits: wire these 4 hooks to your own agent
// system. The default implementation echoes, so the scaffold works end to
// end out of the box.
// ─────────────────────────────────────────────────────────────────────────────

/**
 * 在你的系统中创建一个会话（可选）。返回值会被存进本地会话记录，
 * 后续钩子都能拿到（例如对方系统的 conversation_id）。
 *
 * @param {object} ctx { sessionId, agent: {name, system, model} | null, metadata }
 * @returns {Promise<object>} remoteState — 任意可 JSON 序列化的状态
 */
export async function createRemoteSession(ctx) {
  return { createdAt: Date.now() };
}

/**
 * 把一条用户消息发给你的系统并产出回复。
 *
 * 两种产出方式（二选一或混用）：
 *  - 直接 `return "整段回复文本"`（最简单）；
 *  - 调用 `emit(text)` 逐段输出（平台侧会作为多条 agent.message 展示）。
 *
 * @param {object} ctx { sessionId, remoteState, agent, prompt, history, emit }
 *   history: [{role: "user"|"assistant", text}] 本地保存的完整对话
 * @returns {Promise<string|void>}
 */
export async function sendPrompt(ctx) {
  return `Echo: ${ctx.prompt}`;
}

/**
 * 中断当前运行（如果你的系统支持）。不支持就保持空实现。
 * @param {object} ctx { sessionId, remoteState }
 */
export async function abortRun(ctx) {}

/**
 * 健康检查附加信息（可选）：探测你的系统是否可达。
 * 返回 false 会让 /health 报 not-ok，网关侧注册探活会失败。
 * @returns {Promise<boolean>}
 */
export async function healthy() {
  return true;
}
