import { describe, expect, it } from "vitest";

import {
  isTextPreviewKind,
  parseCsvTable,
  parseMinimalMarkdownLines,
  parseUriList,
  resolveArtifactPreviewKind,
  splitBoldSegments,
} from "./artifact-preview";
import { difyFixture } from "./fixtures/dify";

describe("resolveArtifactPreviewKind", () => {
  it("resolves every documented media type", () => {
    expect(resolveArtifactPreviewKind("text/markdown")).toBe("markdown");
    expect(resolveArtifactPreviewKind("application/json")).toBe("json");
    expect(resolveArtifactPreviewKind("text/csv")).toBe("table");
    expect(resolveArtifactPreviewKind("text/x-code")).toBe("code");
    expect(resolveArtifactPreviewKind("text/plain")).toBe("text");
    expect(resolveArtifactPreviewKind("image/png")).toBe("image");
    expect(resolveArtifactPreviewKind("image/jpeg")).toBe("image");
    expect(resolveArtifactPreviewKind("application/pdf")).toBe("pdf");
    expect(resolveArtifactPreviewKind("text/uri-list")).toBe("link-list");
  });

  it("falls back to download for unrecognized media types", () => {
    expect(resolveArtifactPreviewKind("application/octet-stream")).toBe("download");
    expect(resolveArtifactPreviewKind("application/x-something-unknown")).toBe("download");
  });

  it("is case-insensitive", () => {
    expect(resolveArtifactPreviewKind("IMAGE/PNG")).toBe("image");
  });
});

describe("isTextPreviewKind", () => {
  it("is true for text-ish kinds and false for image/pdf/download", () => {
    expect(isTextPreviewKind("markdown")).toBe(true);
    expect(isTextPreviewKind("json")).toBe(true);
    expect(isTextPreviewKind("table")).toBe(true);
    expect(isTextPreviewKind("code")).toBe(true);
    expect(isTextPreviewKind("text")).toBe(true);
    expect(isTextPreviewKind("image")).toBe(false);
    expect(isTextPreviewKind("pdf")).toBe(false);
    expect(isTextPreviewKind("download")).toBe(false);
    expect(isTextPreviewKind("link-list")).toBe(false);
  });
});

describe("parseCsvTable", () => {
  it("parses the dify fixture's actual CSV artifact content", () => {
    const csv = difyFixture.snapshot.artifacts[0].inline as string;
    const { headers, rows } = parseCsvTable(csv);
    expect(headers).toEqual(["field", "value"]);
    expect(rows).toEqual([["answer", "示例输出"]]);
  });

  it("returns an empty table for empty input", () => {
    expect(parseCsvTable("")).toEqual({ headers: [], rows: [] });
  });

  it("leaves ragged rows as-is rather than rejecting them", () => {
    const { headers, rows } = parseCsvTable("a,b,c\n1,2\n3,4,5,6");
    expect(headers).toEqual(["a", "b", "c"]);
    expect(rows).toEqual([["1", "2"], ["3", "4", "5", "6"]]);
  });
});

describe("parseUriList", () => {
  it("extracts URIs, skipping comments and blank lines", () => {
    const text = "# a comment\nhttps://example.com/a\n\nhttps://example.com/b\n";
    expect(parseUriList(text)).toEqual(["https://example.com/a", "https://example.com/b"]);
  });
});

describe("parseMinimalMarkdownLines", () => {
  it("resolves the a2a fixture's actual markdown artifact content", () => {
    const lines = parseMinimalMarkdownLines("# 执行报告\n\n任务已完成，详情见正文。");
    expect(lines).toEqual([
      { level: 1, text: "执行报告" },
      { level: 0, text: "" },
      { level: 0, text: "任务已完成，详情见正文。" },
    ]);
  });

  it("recognizes heading levels 1 through 3 and treats a bare # differently from ###", () => {
    expect(parseMinimalMarkdownLines("## two\n### three").map((l) => l.level)).toEqual([2, 3]);
  });

  it("does not treat a mid-line hash as a heading", () => {
    expect(parseMinimalMarkdownLines("not # a heading")).toEqual([
      { level: 0, text: "not # a heading" },
    ]);
  });
});

describe("splitBoldSegments", () => {
  it("splits bold runs out of plain text", () => {
    expect(splitBoldSegments("hello **world** plain")).toEqual([
      { bold: false, text: "hello " },
      { bold: true, text: "world" },
      { bold: false, text: " plain" },
    ]);
  });

  it("returns the whole string unbolded when there are no ** markers", () => {
    expect(splitBoldSegments("plain text")).toEqual([{ bold: false, text: "plain text" }]);
  });
});
