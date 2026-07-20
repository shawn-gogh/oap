"use client";

// Dev-only manual verification page for the Run surface (Stage 2). Not
// linked from the sidebar nav — reachable at /dev/run-shell/ directly.
// Lists every fixture (5 providers + 3 shared scenarios) and renders
// RunShell for the selected one, so the identical-structure-across-providers
// acceptance criterion can be eyeballed. This becomes the seed for Stage 7's
// real entry points later.

import { useState } from "react";

import { RunShell } from "@/components/run/RunShell";
import { Button } from "@/components/ui/button";
import { ALL_FIXTURES, FIXTURE_IDS } from "@/lib/run/fixtures";

export default function RunShellDevPage() {
  const [selectedId, setSelectedId] = useState(FIXTURE_IDS[0]);
  const selected = ALL_FIXTURES[selectedId];

  return (
    <div className="mx-auto grid max-w-3xl gap-4 p-6">
      <h1 className="text-lg font-semibold">Run Shell 手动核验（开发用）</h1>
      <div className="flex flex-wrap gap-2">
        {FIXTURE_IDS.map((id) => (
          <Button
            key={id}
            size="sm"
            variant={id === selectedId ? "default" : "outline"}
            onClick={() => setSelectedId(id)}
          >
            {ALL_FIXTURES[id].label}
          </Button>
        ))}
      </div>
      <RunShell key={selected.snapshot.runId} runId={selected.snapshot.runId} />
    </div>
  );
}
