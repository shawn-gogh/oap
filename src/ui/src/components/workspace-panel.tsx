"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { Download, FolderOpen, RefreshCw, Trash2, Upload, X } from "lucide-react";
import {
  deleteWorkspaceFile,
  listWorkspaceFiles,
  uploadWorkspaceFile,
  workspaceFileDownloadUrl,
} from "@/lib/api";
import type { WorkspaceFile } from "@/lib/types";

function formatDate(ms: number | null): string {
  if (!ms) return "—";
  try {
    return new Intl.DateTimeFormat(undefined, {
      month: "short",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    }).format(new Date(ms));
  } catch {
    return "—";
  }
}

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "0 B";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  for (const unit of units) {
    if (value < 1024 || unit === units[units.length - 1]) return `${value.toFixed(1)} ${unit}`;
    value /= 1024;
  }
  return `${value.toFixed(1)} GB`;
}

export function WorkspacePanel({
  sessionId,
  onClose,
}: {
  sessionId: string;
  onClose: () => void;
}) {
  const [files, setFiles] = useState<WorkspaceFile[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [uploading, setUploading] = useState(false);
  const [busyPath, setBusyPath] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const list = await listWorkspaceFiles(sessionId);
      setFiles(list.sort((a, b) => a.path.localeCompare(b.path)));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [sessionId]);

  useEffect(() => {
    void load();
  }, [load]);

  const handleUpload = async (fileList: FileList | null) => {
    if (!fileList || fileList.length === 0) return;
    setUploading(true);
    setError(null);
    try {
      for (const file of Array.from(fileList)) {
        await uploadWorkspaceFile(sessionId, file, file.name);
      }
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setUploading(false);
      if (fileInputRef.current) fileInputRef.current.value = "";
    }
  };

  const handleDownload = async (path: string) => {
    setBusyPath(path);
    try {
      const url = await workspaceFileDownloadUrl(sessionId, path);
      // window.open here would be popup-blocked: the presign round-trip
      // consumes the user-gesture window. A synthesized anchor click is not.
      const a = document.createElement("a");
      a.href = url;
      a.download = path.split("/").pop() ?? path;
      a.target = "_blank";
      a.rel = "noopener noreferrer";
      document.body.appendChild(a);
      a.click();
      a.remove();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyPath(null);
    }
  };

  const handleDelete = async (path: string) => {
    setBusyPath(path);
    try {
      await deleteWorkspaceFile(sessionId, path);
      setFiles((prev) => prev.filter((f) => f.path !== path));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyPath(null);
    }
  };

  return (
    <aside className="flex h-screen min-h-0 w-[420px] shrink-0 flex-col border-l border-border bg-background">
      <header className="flex h-12 shrink-0 items-center gap-2 border-b border-border px-4">
        <FolderOpen className="size-3.5 text-muted-foreground" />
        <span className="text-[13px] font-medium">workspace</span>
        <button
          type="button"
          onClick={() => void load()}
          disabled={loading}
          className="ml-auto rounded p-1 hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
          title="Refresh"
          aria-label="Refresh workspace files"
        >
          <RefreshCw className={`size-3.5 text-muted-foreground ${loading ? "animate-spin" : ""}`} />
        </button>
        <button
          type="button"
          onClick={onClose}
          className="rounded p-1 hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
          title="Close workspace"
          aria-label="Close workspace"
        >
          <X className="size-4 text-muted-foreground" />
        </button>
      </header>

      <div className="flex items-center gap-2 border-b border-border bg-muted/30 px-4 py-2">
        <input
          ref={fileInputRef}
          type="file"
          multiple
          className="hidden"
          onChange={(e) => void handleUpload(e.target.files)}
        />
        <button
          type="button"
          onClick={() => fileInputRef.current?.click()}
          disabled={uploading}
          className="inline-flex items-center gap-1.5 rounded-md border border-border bg-card px-2.5 py-1 text-[11px] font-medium transition-colors hover:bg-muted disabled:pointer-events-none disabled:opacity-50"
        >
          <Upload className="size-3.5" />
          {uploading ? "Uploading…" : "Upload"}
        </button>
        <span className="ml-auto font-mono text-[11px] text-muted-foreground">
          {files.length} file{files.length === 1 ? "" : "s"}
        </span>
      </div>

      {error && (
        <div className="border-b border-border bg-red-500/10 px-4 py-2 text-[11px] text-red-600 dark:text-red-400">
          {error}
        </div>
      )}

      <div className="min-h-0 flex-1 divide-y divide-border overflow-y-auto">
        {loading && files.length === 0 ? (
          <div className="p-4 text-xs text-muted-foreground">Loading…</div>
        ) : files.length === 0 ? (
          <div className="flex flex-col items-center gap-2 p-8 text-center">
            <FolderOpen className="size-6 text-muted-foreground/60" />
            <p className="text-xs font-medium">No files yet</p>
            <p className="text-[11px] text-muted-foreground">
              Upload documents, code, or images for the agent to work with.
            </p>
          </div>
        ) : (
          files.map((file) => (
            <div key={file.path} className="flex items-center gap-2 px-4 py-2.5">
              <div className="min-w-0 flex-1">
                <p className="truncate font-mono text-xs" title={file.path}>
                  {file.path}
                </p>
                <p className="mt-0.5 text-[11px] text-muted-foreground">
                  {formatBytes(file.size_bytes)} · {formatDate(file.updated_at)}
                </p>
              </div>
              <button
                type="button"
                onClick={() => void handleDownload(file.path)}
                disabled={busyPath === file.path}
                className="rounded p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:pointer-events-none disabled:opacity-50"
                title="Download"
                aria-label={`Download ${file.path}`}
              >
                <Download className="size-3.5" />
              </button>
              <button
                type="button"
                onClick={() => void handleDelete(file.path)}
                disabled={busyPath === file.path}
                className="rounded p-1 text-muted-foreground transition-colors hover:bg-red-500/10 hover:text-red-600 disabled:pointer-events-none disabled:opacity-50 dark:hover:text-red-400"
                title="Delete"
                aria-label={`Delete ${file.path}`}
              >
                <Trash2 className="size-3.5" />
              </button>
            </div>
          ))
        )}
      </div>

      <footer className="border-t border-border px-4 py-1.5 font-mono text-[11px] text-muted-foreground">
        GET /session/{sessionId}/workspace/files
      </footer>
    </aside>
  );
}
