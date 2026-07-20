"use client";

// Dev-only manual verification page for the Run surface (Stages 2-3-7). Not
// linked from the sidebar nav — reachable at /dev/run-shell/ directly.
//
// Top section (Stage 7): paste a real session id and connect real-client.ts
// to the live backend — the actual end-to-end proof the real transport +
// adapter work, not just typechecking.
//
// Middle section: pick an agent template and submit its input form
// (Stage 3), then view the resulting fixture-backed run below via the same
// RunShell used everywhere else — closing the create -> view loop.
//
// Bottom section: lists every pre-seeded fixture (5 providers + 3 shared
// scenarios) and renders RunShell for the selected one, so the
// identical-structure-across-providers acceptance criterion can be
// eyeballed.

import { useState } from "react";

import { RunInputForm } from "@/components/run/RunInputForm";
import { RunShell } from "@/components/run/RunShell";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { apiErrorMessage, getActiveTurn } from "@/lib/api";
import { ALL_FIXTURES, FIXTURE_IDS } from "@/lib/run/fixtures";
import { RUN_AGENT_TEMPLATES } from "@/lib/run/fixtures/templates";
import { createRealRunTransport } from "@/lib/run/real-client";

export default function RunShellDevPage() {
  const [selectedTemplateId, setSelectedTemplateId] = useState(RUN_AGENT_TEMPLATES[0].agentId);
  const [createdRunId, setCreatedRunId] = useState<string | null>(null);
  const selectedTemplate = RUN_AGENT_TEMPLATES.find((t) => t.agentId === selectedTemplateId)!;

  const [selectedFixtureId, setSelectedFixtureId] = useState(FIXTURE_IDS[0]);
  const selectedFixture = ALL_FIXTURES[selectedFixtureId];

  const [realSessionIdInput, setRealSessionIdInput] = useState("");
  const [realTurnIdInput, setRealTurnIdInput] = useState("");
  const [connecting, setConnecting] = useState(false);
  const [connectError, setConnectError] = useState<string | null>(null);
  const [realConnection, setRealConnection] = useState<{ sessionId: string; turnId: string } | null>(
    null,
  );

  const connectRealSession = async () => {
    const sessionId = realSessionIdInput.trim();
    if (!sessionId) return;
    setConnecting(true);
    setConnectError(null);
    try {
      // A turn id typed directly skips the active-turn lookup — useful for
      // connecting to a specific turn without racing very fast models that
      // complete before getActiveTurn's own round trip resolves.
      const explicitTurnId = realTurnIdInput.trim();
      if (explicitTurnId) {
        setRealConnection({ sessionId, turnId: explicitTurnId });
        return;
      }
      const active = await getActiveTurn(sessionId);
      if (!active) {
        setConnectError(
          "该会话当前没有活跃的 Turn。请先在 /chat/ 页面对这个会话发一条消息，再回来连接，或直接填写 Turn id。",
        );
        return;
      }
      setRealConnection({ sessionId, turnId: active.turn.id });
    } catch (e) {
      setConnectError(apiErrorMessage(e, "连接失败"));
    } finally {
      setConnecting(false);
    }
  };

  return (
    <div className="mx-auto grid max-w-3xl gap-8 p-6">
      <section className="grid gap-3">
        <h1 className="text-lg font-semibold">连接真实会话（Stage 7 手动核验）</h1>
        <p className="text-sm text-muted-foreground">
          先在 <code>/chat/</code> 页面用任意已配置的智能体开一个会话并发一条消息，
          把该会话的 id 粘贴到下面，验证真实的 Run 传输层（real-client.ts）。
        </p>
        <div className="flex gap-2">
          <Input
            value={realSessionIdInput}
            onChange={(event) => setRealSessionIdInput(event.target.value)}
            placeholder="ses_..."
            className="max-w-xs"
          />
          <Input
            value={realTurnIdInput}
            onChange={(event) => setRealTurnIdInput(event.target.value)}
            placeholder="turn_...（可选，跳过活跃 Turn 查找）"
            className="max-w-xs"
          />
          <Button size="sm" disabled={connecting} onClick={() => void connectRealSession()}>
            {connecting ? "连接中…" : "连接"}
          </Button>
        </div>
        {connectError && <p className="text-sm text-destructive">{connectError}</p>}
        {realConnection && (
          <RunShell
            key={`${realConnection.sessionId}:${realConnection.turnId}`}
            runId={realConnection.turnId}
            transport={createRealRunTransport(realConnection.sessionId)}
          />
        )}
      </section>
      <section className="grid gap-3 border-t border-border pt-6">
        <h2 className="text-lg font-semibold">创建 Run（Stage 3 手动核验）</h2>
        <div className="flex flex-wrap gap-2">
          {RUN_AGENT_TEMPLATES.map((template) => (
            <Button
              key={template.agentId}
              size="sm"
              variant={template.agentId === selectedTemplateId ? "default" : "outline"}
              onClick={() => {
                setSelectedTemplateId(template.agentId);
                setCreatedRunId(null);
              }}
            >
              {template.agentName}
            </Button>
          ))}
        </div>
        <RunInputForm
          key={selectedTemplate.agentId}
          agentId={selectedTemplate.agentId}
          agentName={selectedTemplate.agentName}
          schema={selectedTemplate.inputSchema}
          onCreated={setCreatedRunId}
        />
        {createdRunId && <RunShell key={createdRunId} runId={createdRunId} />}
      </section>

      <section className="grid gap-3 border-t border-border pt-6">
        <h2 className="text-lg font-semibold">Run Shell 手动核验（开发用）</h2>
        <div className="flex flex-wrap gap-2">
          {FIXTURE_IDS.map((id) => (
            <Button
              key={id}
              size="sm"
              variant={id === selectedFixtureId ? "default" : "outline"}
              onClick={() => setSelectedFixtureId(id)}
            >
              {ALL_FIXTURES[id].label}
            </Button>
          ))}
        </div>
        <RunShell key={selectedFixture.snapshot.runId} runId={selectedFixture.snapshot.runId} />
      </section>
    </div>
  );
}
