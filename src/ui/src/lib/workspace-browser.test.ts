import { describe, expect, it } from "vitest";

import type { WorkspaceFile } from "@/lib/types";
import {
  buildWorkspaceTree,
  buildWorkspaceTreeFromFolders,
  conflictingWorkspacePaths,
  listWorkspaceEntries,
  searchWorkspaceFiles,
  workspaceBreadcrumbs,
  workspaceAgentTaskPrompt,
  workspaceConversationReference,
  workspacePreviewKind,
  workspaceTransferConflicts,
  workspaceTransferDestination,
  workspaceUploadPath,
} from "./workspace-browser";

const files: WorkspaceFile[] = [
  { path: "README.md", size_bytes: 10, updated_at: 1 },
  { path: "reports/2026/july.csv", size_bytes: 20, updated_at: 2 },
  { path: "reports/summary.pdf", size_bytes: 30, updated_at: 3 },
  { path: "images/chart.png", size_bytes: 40, updated_at: 4 },
];

describe("workspace browser model", () => {
  it("builds a virtual directory tree from object paths", () => {
    const tree = buildWorkspaceTree(files);
    expect(tree.children.map((child) => child.path)).toEqual(["images", "reports"]);
    expect(tree.children.find((child) => child.path === "reports")?.totalFileCount).toBe(2);
    expect(tree.children.find((child) => child.path === "reports")?.children[0].path).toBe(
      "reports/2026",
    );
  });

  it("lists only direct files and immediate folders", () => {
    expect(listWorkspaceEntries(files, "reports")).toEqual([
      { kind: "folder", name: "2026", path: "reports/2026", fileCount: 1 },
      { kind: "file", name: "summary.pdf", file: files[2] },
    ]);
  });

  it("keeps empty folders visible in the tree and directory listing", () => {
    const tree = buildWorkspaceTreeFromFolders(["空目录", "reports/待处理"], files);
    expect(tree.children.map((child) => child.path)).toContain("空目录");
    expect(tree.children.find((child) => child.path === "reports")?.children.map((child) => child.path)).toContain(
      "reports/待处理",
    );
    expect(listWorkspaceEntries([], "reports", ["reports/待处理"])).toEqual([
      { kind: "folder", name: "待处理", path: "reports/待处理", fileCount: 0 },
    ]);
  });

  it("creates navigable breadcrumbs and searches full paths", () => {
    expect(workspaceBreadcrumbs("reports/2026")).toEqual([
      { name: "我的文件", path: "" },
      { name: "reports", path: "reports" },
      { name: "2026", path: "reports/2026" },
    ]);
    expect(searchWorkspaceFiles(files, "JULY")).toEqual([files[1]]);
  });

  it("does not directly preview active or unsupported formats", () => {
    expect(workspacePreviewKind("chart.png")).toBe("image");
    expect(workspacePreviewKind("report.pdf")).toBe("pdf");
    expect(workspacePreviewKind("page.html")).toBe("unsupported");
    expect(workspacePreviewKind("vector.svg")).toBe("unsupported");
  });

  it("plans uploads in the current directory and detects exact conflicts", () => {
    const paths = [
      workspaceUploadPath("reports/2026", "july.csv"),
      workspaceUploadPath("reports/2026", "august.csv"),
    ];
    expect(paths).toEqual(["reports/2026/july.csv", "reports/2026/august.csv"]);
    expect(conflictingWorkspacePaths(files, paths)).toEqual(["reports/2026/july.csv"]);
  });

  it("plans file and folder destinations and reports collisions", () => {
    expect(workspaceTransferDestination("reports/summary.pdf", "archive")).toBe(
      "archive/summary.pdf",
    );
    expect(workspaceTransferConflicts(files, ["reports/summary.pdf"], "archive")).toEqual([]);
    expect(
      workspaceTransferConflicts(
        [...files, { path: "archive/summary.pdf", size_bytes: 1, updated_at: 1 }],
        ["reports/summary.pdf"],
        "archive",
      ),
    ).toEqual(["archive/summary.pdf"]);
  });

  it("creates stable workspace context for chat and agent tasks", () => {
    expect(workspaceConversationReference(["reports/b.csv", "reports/a.csv"])).toBe(
      "请参考以下会话工作区路径：\n- `reports/a.csv`\n- `reports/b.csv`",
    );
    expect(workspaceAgentTaskPrompt(["reports/a.csv"])).toContain(
      "完成后请列出使用过的文件和产生的新文件",
    );
  });
});
