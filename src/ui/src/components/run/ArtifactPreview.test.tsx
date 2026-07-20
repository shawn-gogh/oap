import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

// Same rationale as RunShell.test.tsx: no jsdom in this repo's vitest setup,
// so the "dispatch by media type only, never by provider" acceptance
// criterion is checked directly against the source text.

const SOURCE = readFileSync(fileURLToPath(new URL("./ArtifactPreview.tsx", import.meta.url)), "utf8");

describe("ArtifactPreview", () => {
  it("dispatches preview rendering only through resolveArtifactPreviewKind, never on providerName", () => {
    expect(SOURCE).toContain("resolveArtifactPreviewKind(artifact.mediaType)");
    expect(SOURCE).not.toMatch(/\.providerName/);
    expect(SOURCE).not.toMatch(/provider\s*===\s*["']/);
  });
});
