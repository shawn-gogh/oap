"use client";

// Dev-only manual verification page for the Run surface (Stages 2-3). Not
// linked from the sidebar nav — reachable at /dev/run-shell/ directly.
//
// Top section: pick an agent template and submit its input form (Stage 3),
// then view the resulting run below via the same RunShell used for the
// pre-seeded fixtures — closing the create -> view loop.
//
// Bottom section: lists every pre-seeded fixture (5 providers + 3 shared
// scenarios) and renders RunShell for the selected one, so the
// identical-structure-across-providers acceptance criterion can be
// eyeballed. This becomes the seed for Stage 7's real entry points later.

import { useState } from "react";

import { RunInputForm } from "@/components/run/RunInputForm";
import { RunShell } from "@/components/run/RunShell";
import { Button } from "@/components/ui/button";
import { ALL_FIXTURES, FIXTURE_IDS } from "@/lib/run/fixtures";
import { RUN_AGENT_TEMPLATES } from "@/lib/run/fixtures/templates";

export default function RunShellDevPage() {
  const [selectedTemplateId, setSelectedTemplateId] = useState(RUN_AGENT_TEMPLATES[0].agentId);
  const [createdRunId, setCreatedRunId] = useState<string | null>(null);
  const selectedTemplate = RUN_AGENT_TEMPLATES.find((t) => t.agentId === selectedTemplateId)!;

  const [selectedFixtureId, setSelectedFixtureId] = useState(FIXTURE_IDS[0]);
  const selectedFixture = ALL_FIXTURES[selectedFixtureId];

  return (
    <div className="mx-auto grid max-w-3xl gap-8 p-6">
      <section className="grid gap-3">
        <h1 className="text-lg font-semibold">创建 Run（Stage 3 手动核验）</h1>
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
