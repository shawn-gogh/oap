"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  CheckCircle2,
  CircleAlert,
  CircleX,
  Copy,
  Download,
  File,
  FileCode2,
  FileImage,
  FileJson,
  FileQuestion,
  FileSpreadsheet,
  FileText,
  Folder,
  FolderPlus,
  FolderOpen,
  FolderInput,
  Grid2X2,
  List,
  Loader2,
  MessageSquarePlus,
  Pencil,
  RefreshCw,
  RotateCcw,
  Search,
  Sparkles,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import { HighlightedCode } from "@/components/code-block";
import { useConfirm } from "@/components/confirm-dialog";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  batchTransferWorkspacePaths,
  browseWorkspaceFiles,
  createWorkspaceFolder,
  deleteWorkspaceTrash,
  emptyWorkspaceTrash,
  listWorkspaceTrash,
  listWorkspaceFolders,
  moveWorkspacePath,
  restoreWorkspaceTrash,
  trashWorkspacePaths,
  uploadWorkspaceFile,
  workspaceFileDownloadUrl,
} from "@/lib/api";
import type { WorkspaceFile, WorkspaceTrashItem } from "@/lib/types";
import {
  buildWorkspaceTreeFromFolders,
  conflictingWorkspacePaths,
  listWorkspaceEntries,
  parentWorkspacePath,
  workspaceBreadcrumbs,
  workspaceCodeLanguage,
  workspaceFileName,
  workspacePreviewKind,
  workspaceTransferConflicts,
  workspaceTransferDestination,
  workspaceUploadPath,
  type WorkspaceDirectory,
  type WorkspaceEntry,
} from "@/lib/workspace-browser";

const MAX_TEXT_PREVIEW_BYTES = 2 * 1024 * 1024;
const WORKSPACE_DRAG_TYPE = "application/x-oap-workspace-path";

type UploadStatus = "queued" | "uploading" | "success" | "error" | "cancelled" | "skipped";

interface WorkspaceUploadItem {
  id: string;
  file: File;
  path: string;
  status: UploadStatus;
  progress: number;
  error?: string;
}

function formatDate(ms: number | null): string {
  if (!ms) return "暂无时间";
  try {
    return new Intl.DateTimeFormat("zh-CN", {
      month: "short",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    }).format(new Date(ms));
  } catch {
    return "暂无时间";
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
  onInsertPaths,
  onProcessPaths,
}: {
  sessionId: string;
  onClose: () => void;
  onInsertPaths: (paths: string[]) => void;
  onProcessPaths: (paths: string[]) => Promise<void>;
}) {
  const confirmAction = useConfirm();
  const [files, setFiles] = useState<WorkspaceFile[]>([]);
  const [folderPaths, setFolderPaths] = useState<string[]>([]);
  const [page, setPage] = useState(0);
  const [totalFiles, setTotalFiles] = useState(0);
  const [hasNextPage, setHasNextPage] = useState(false);
  const [sortBy, setSortBy] = useState<"name" | "size" | "updated">("name");
  const [sortDirection, setSortDirection] = useState<"asc" | "desc">("asc");
  const [newFolderOpen, setNewFolderOpen] = useState(false);
  const [newFolderName, setNewFolderName] = useState("");
  const [currentPath, setCurrentPath] = useState("");
  const [query, setQuery] = useState("");
  const [view, setView] = useState<"list" | "grid">("list");
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set([""]));
  const [previewFile, setPreviewFile] = useState<WorkspaceFile | null>(null);
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [renameSource, setRenameSource] = useState<string | null>(null);
  const [renameName, setRenameName] = useState("");
  const [transferMode, setTransferMode] = useState<"move" | "copy" | null>(null);
  const [transferDestination, setTransferDestination] = useState("");
  const [operationBusy, setOperationBusy] = useState(false);
  const [operationStatus, setOperationStatus] = useState<string | null>(null);
  const [operationError, setOperationError] = useState<string | null>(null);
  const [processingPaths, setProcessingPaths] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [uploadItems, setUploadItems] = useState<WorkspaceUploadItem[]>([]);
  const [dragging, setDragging] = useState(false);
  const [busyPath, setBusyPath] = useState<string | null>(null);
  const [trashMode, setTrashMode] = useState(false);
  const [trashItems, setTrashItems] = useState<WorkspaceTrashItem[]>([]);
  const [selectedTrashIds, setSelectedTrashIds] = useState<Set<string>>(new Set());
  const fileInputRef = useRef<HTMLInputElement>(null);
  const uploadControllersRef = useRef(new Map<string, AbortController>());
  const cancelledUploadsRef = useRef(new Set<string>());
  const dragDepthRef = useRef(0);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [result, folders] = await Promise.all([
        browseWorkspaceFiles(sessionId, {
          prefix: currentPath,
          query,
          cursor: page * 50,
          limit: 50,
          sortBy,
          direction: sortDirection,
        }),
        listWorkspaceFolders(sessionId),
      ]);
      setFiles(result.files);
      setFolderPaths(folders);
      setTotalFiles(result.total);
      setHasNextPage(result.next_cursor !== null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [currentPath, page, query, sessionId, sortBy, sortDirection]);

  const loadTrash = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setTrashItems(await listWorkspaceTrash(sessionId));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [sessionId]);

  useEffect(() => {
    if (trashMode) void loadTrash();
    else void load();
  }, [load, loadTrash, trashMode]);

  useEffect(() => {
    const controllers = uploadControllersRef.current;
    return () => controllers.forEach((controller) => controller.abort());
  }, []);

  const tree = useMemo(() => buildWorkspaceTreeFromFolders(folderPaths, files), [files, folderPaths]);
  const searchResults = query.trim() ? files : [];
  const entries = useMemo(
    () => (query.trim() ? [] : listWorkspaceEntries(files, currentPath, folderPaths)),
    [currentPath, files, folderPaths, query],
  );
  const breadcrumbs = workspaceBreadcrumbs(currentPath);
  const uploadBusy = uploadItems.some(
    (item) => item.status === "queued" || item.status === "uploading",
  );

  const navigate = (path: string) => {
    setTrashMode(false);
    setCurrentPath(path);
    setPage(0);
    setQuery("");
    setSelectedPaths(new Set());
    setExpandedPaths((current) => {
      const next = new Set(current);
      next.add("");
      const segments = path.split("/").filter(Boolean);
      segments.forEach((_, index) => next.add(segments.slice(0, index + 1).join("/")));
      return next;
    });
  };

  const openTrash = () => {
    setTrashMode(true);
    setSelectedPaths(new Set());
    setSelectedTrashIds(new Set());
    setPreviewFile(null);
    setQuery("");
  };

  const updateUploadItem = (id: string, patch: Partial<WorkspaceUploadItem>) => {
    setUploadItems((current) =>
      current.map((item) => (item.id === id ? { ...item, ...patch } : item)),
    );
  };

  const uploadOne = async (item: WorkspaceUploadItem) => {
    if (cancelledUploadsRef.current.has(item.id)) {
      updateUploadItem(item.id, { status: "cancelled" });
      return;
    }
    const controller = new AbortController();
    uploadControllersRef.current.set(item.id, controller);
    updateUploadItem(item.id, { status: "uploading", progress: 0, error: undefined });
    try {
      await uploadWorkspaceFile(sessionId, item.file, item.path, {
        signal: controller.signal,
        onProgress: (loaded, total) =>
          updateUploadItem(item.id, {
            progress: total > 0 ? Math.min(100, Math.round((loaded / total) * 100)) : 0,
          }),
      });
      updateUploadItem(item.id, { status: "success", progress: 100 });
    } catch (reason) {
      if (reason instanceof DOMException && reason.name === "AbortError") {
        updateUploadItem(item.id, { status: "cancelled" });
      } else {
        updateUploadItem(item.id, {
          status: "error",
          error: reason instanceof Error ? reason.message : String(reason),
        });
      }
    } finally {
      uploadControllersRef.current.delete(item.id);
    }
  };

  const startUploads = async (selectedFiles: File[]) => {
    if (selectedFiles.length === 0) return;
    if (uploadBusy) {
      setError("当前已有上传任务，请等待完成或取消后再添加文件。");
      return;
    }
    setError(null);
    try {
      const candidates = selectedFiles.map((file, index) => ({
        id: `${Date.now()}-${index}-${Math.random().toString(36).slice(2)}`,
        file,
        path: workspaceUploadPath(currentPath, file.name),
        status: "queued" as UploadStatus,
        progress: 0,
      }));
      const conflicts = new Set(
        conflictingWorkspacePaths(files, candidates.map((candidate) => candidate.path)),
      );
      const overwrite =
        conflicts.size === 0 ||
        (await confirmAction({
          title: `发现 ${conflicts.size} 个同名文件`,
          description: `继续将覆盖：${[...conflicts].slice(0, 3).join("、")}${conflicts.size > 3 ? " 等文件" : ""}。取消将跳过同名文件，其余文件仍会上传。`,
          confirmLabel: "覆盖并上传",
          cancelLabel: "跳过同名文件",
          destructive: false,
        }));
      const planned = candidates.map((candidate) =>
        conflicts.has(candidate.path) && !overwrite
          ? { ...candidate, status: "skipped" as UploadStatus }
          : candidate,
      );
      setUploadItems((current) => [
        ...current.filter((item) => item.status === "uploading" || item.status === "queued"),
        ...planned,
      ]);
      for (const item of planned) {
        if (item.status === "queued") await uploadOne(item);
      }
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      if (fileInputRef.current) fileInputRef.current.value = "";
    }
  };

  const cancelUpload = (id: string) => {
    cancelledUploadsRef.current.add(id);
    uploadControllersRef.current.get(id)?.abort();
    updateUploadItem(id, { status: "cancelled" });
  };

  const retryUpload = async (item: WorkspaceUploadItem) => {
    cancelledUploadsRef.current.delete(item.id);
    await uploadOne(item);
    await load();
  };

  const handleDownload = async (path: string) => {
    setBusyPath(path);
    try {
      const url = await workspaceFileDownloadUrl(sessionId, path);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = workspaceFileName(path);
      anchor.target = "_blank";
      anchor.rel = "noopener noreferrer";
      document.body.appendChild(anchor);
      anchor.click();
      anchor.remove();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyPath(null);
    }
  };

  const handleDelete = async (path: string) => {
    const confirmed = await confirmAction({
      title: "移至回收站？",
      description: "文件或目录中的全部内容将移至回收站，并保留 30 天。",
      confirmLabel: "移至回收站",
    });
    if (!confirmed) return;
    setBusyPath(path);
    try {
      await trashWorkspacePaths(sessionId, [path]);
      setFiles((current) =>
        current.filter((file) => file.path !== path && !file.path.startsWith(`${path}/`)),
      );
      setSelectedPaths((current) => {
        const next = new Set(current);
        next.delete(path);
        return next;
      });
      if (previewFile?.path === path) setPreviewFile(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyPath(null);
    }
  };

  const handleBatchDelete = async () => {
    const paths = [...selectedPaths];
    if (paths.length === 0) return;
    const confirmed = await confirmAction({
      title: `将所选 ${paths.length} 项移至回收站？`,
      description: "所选文件以及目录中的全部内容将保留 30 天，期间可以还原。",
      confirmLabel: "移至回收站",
    });
    if (!confirmed) return;
    setOperationBusy(true);
    setError(null);
    try {
      const affected = await trashWorkspacePaths(sessionId, paths);
      setOperationStatus(`已移至回收站，共处理 ${affected} 个对象。`);
      setSelectedPaths(new Set());
      await load();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setOperationBusy(false);
    }
  };

  const handleRestoreTrash = async () => {
    const ids = [...selectedTrashIds];
    if (ids.length === 0) return;
    setOperationBusy(true);
    setError(null);
    try {
      let affected: number;
      try {
        affected = await restoreWorkspaceTrash(sessionId, ids);
      } catch (reason) {
        const message = reason instanceof Error ? reason.message : String(reason);
        if (!message.includes("原位置已有同名文件")) throw reason;
        const overwrite = await confirmAction({
          title: "原位置存在同名文件",
          description: "还原失败，可能是原位置已有同名文件。是否覆盖原位置并继续还原？",
          confirmLabel: "覆盖并还原",
          cancelLabel: "取消",
          destructive: false,
        });
        if (!overwrite) throw reason;
        affected = await restoreWorkspaceTrash(sessionId, ids, true);
      }
      setSelectedTrashIds(new Set());
      setOperationStatus(`已还原，共处理 ${affected} 个对象。`);
      await loadTrash();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setOperationBusy(false);
    }
  };

  const handleDeleteTrash = async () => {
    const ids = [...selectedTrashIds];
    if (ids.length === 0) return;
    const confirmed = await confirmAction({
      title: `永久删除所选 ${ids.length} 项？`,
      description: "永久删除后无法恢复，请确认这些文件不再需要。",
      confirmLabel: "永久删除",
    });
    if (!confirmed) return;
    setOperationBusy(true);
    setError(null);
    try {
      await deleteWorkspaceTrash(sessionId, ids);
      setSelectedTrashIds(new Set());
      await loadTrash();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setOperationBusy(false);
    }
  };

  const handleEmptyTrash = async () => {
    if (trashItems.length === 0) return;
    const confirmed = await confirmAction({
      title: "清空回收站？",
      description: `回收站中的 ${trashItems.length} 项将被永久删除，且无法恢复。`,
      confirmLabel: "清空回收站",
    });
    if (!confirmed) return;
    setOperationBusy(true);
    setError(null);
    try {
      await emptyWorkspaceTrash(sessionId);
      setSelectedTrashIds(new Set());
      await loadTrash();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setOperationBusy(false);
    }
  };

  const openRename = () => {
    const source = [...selectedPaths][0];
    if (!source || selectedPaths.size !== 1) return;
    setRenameSource(source);
    setRenameName(workspaceFileName(source));
    setOperationError(null);
  };

  const commitRename = async () => {
    if (!renameSource) return;
    const name = renameName.trim();
    if (!name || name === "." || name === ".." || name.includes("/")) {
      setOperationError("名称不能为空，且不能包含斜杠或使用 .、..。");
      return;
    }
    const parent = parentWorkspacePath(renameSource);
    const destination = parent ? `${parent}/${name}` : name;
    if (destination === renameSource) {
      setRenameSource(null);
      return;
    }
    const sourceIsFile = files.some((file) => file.path === renameSource);
    const hasConflict = files.some((file) =>
      sourceIsFile
        ? file.path === destination
        : file.path === destination || file.path.startsWith(`${destination}/`),
    );
    const overwrite =
      !hasConflict ||
      (await confirmAction({
        title: "目标名称已存在",
        description: "继续将覆盖目标位置中的同名文件。",
        confirmLabel: "覆盖并重命名",
        destructive: false,
      }));
    if (hasConflict && !overwrite) return;
    setOperationBusy(true);
    setOperationError(null);
    try {
      await moveWorkspacePath(sessionId, renameSource, destination, overwrite);
      setRenameSource(null);
      setSelectedPaths(new Set());
      await load();
    } catch (reason) {
      setOperationError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setOperationBusy(false);
    }
  };

  const openTransfer = (mode: "move" | "copy") => {
    if (selectedPaths.size === 0) return;
    setTransferDestination("");
    setTransferMode(mode);
    setOperationError(null);
  };

  const commitTransfer = async () => {
    if (!transferMode || selectedPaths.size === 0) return;
    const sources = [...selectedPaths];
    const destinations = sources.map((source) =>
      workspaceTransferDestination(source, transferDestination),
    );
    if (new Set(destinations).size !== destinations.length) {
      setOperationError("所选项目在目标目录中会产生重名，请分批操作。");
      return;
    }
    if (
      sources.some(
        (source, index) =>
          destinations[index] === source || transferDestination.startsWith(`${source}/`),
      )
    ) {
      setOperationError("不能移动或复制到原位置或自身的子目录。");
      return;
    }
    const conflicts = workspaceTransferConflicts(files, sources, transferDestination);
    const overwrite =
      conflicts.length === 0 ||
      (await confirmAction({
        title: `目标目录存在 ${conflicts.length} 个同名文件`,
        description: `继续将覆盖：${conflicts.slice(0, 3).join("、")}${conflicts.length > 3 ? " 等文件" : ""}。`,
        confirmLabel: transferMode === "move" ? "覆盖并移动" : "覆盖并复制",
        destructive: false,
      }));
    if (conflicts.length > 0 && !overwrite) return;
    setOperationBusy(true);
    setOperationStatus(`正在${transferMode === "move" ? "移动" : "复制"} ${sources.length} 项…`);
    setOperationError(null);
    try {
      const affected = await batchTransferWorkspacePaths(
        sessionId,
        sources,
        transferDestination,
        transferMode,
        overwrite,
      );
      setOperationStatus(`已完成，共处理 ${affected} 个对象。`);
      setTransferMode(null);
      setSelectedPaths(new Set());
      await load();
    } catch (reason) {
      setOperationError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setOperationBusy(false);
    }
  };

  const handleInternalMove = async (source: string, destination: string) => {
    const target = workspaceTransferDestination(source, destination);
    if (source === target || destination.startsWith(`${source}/`)) {
      setError("不能移动到原位置或自身的子目录。");
      return;
    }
    setOperationBusy(true);
    setOperationStatus(
      `正在将“${workspaceFileName(source)}”移动到${destination ? `“${destination}”` : "“我的文件”"}…`,
    );
    setError(null);
    try {
      const affected = await batchTransferWorkspacePaths(
        sessionId,
        [source],
        destination,
        "move",
        false,
      );
      setOperationStatus(`移动完成，共处理 ${affected} 个对象。`);
      setSelectedPaths(new Set());
      await load();
    } catch (reason) {
      setError(`移动失败：${reason instanceof Error ? reason.message : String(reason)}`);
    } finally {
      setOperationBusy(false);
    }
  };

  const handleCreateFolder = async () => {
    const name = newFolderName.trim();
    if (!name || name === "." || name === ".." || name.includes("/")) {
      setOperationError("文件夹名称不能为空，且不能包含斜杠或使用 .、..。");
      return;
    }
    const path = currentPath ? `${currentPath}/${name}` : name;
    if (folderPaths.includes(path)) {
      setOperationError("当前目录已存在同名文件夹。");
      return;
    }
    setOperationBusy(true);
    setOperationError(null);
    setOperationStatus(`正在创建文件夹“${name}”…`);
    try {
      await createWorkspaceFolder(sessionId, path);
      setNewFolderOpen(false);
      setNewFolderName("");
      setOperationStatus(`文件夹“${name}”已创建。`);
      await load();
    } catch (reason) {
      setOperationError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setOperationBusy(false);
    }
  };

  const insertPathsIntoConversation = (paths: string[]) => {
    if (paths.length === 0) return;
    onInsertPaths(paths);
    setSelectedPaths(new Set());
    onClose();
  };

  const processWorkspacePaths = async (paths: string[]) => {
    if (paths.length === 0 || processingPaths) return;
    setProcessingPaths(true);
    setError(null);
    try {
      await onProcessPaths(paths);
      setSelectedPaths(new Set());
      setPreviewFile(null);
      onClose();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setProcessingPaths(false);
    }
  };

  return (
    <aside
      onDragEnter={(event) => {
        if (trashMode) return;
        if (!event.dataTransfer.types.includes("Files")) return;
        event.preventDefault();
        dragDepthRef.current += 1;
        setDragging(true);
      }}
      onDragOver={(event) => {
        if (
          !event.dataTransfer.types.includes("Files") &&
          !event.dataTransfer.types.includes(WORKSPACE_DRAG_TYPE)
        ) return;
        event.preventDefault();
        event.dataTransfer.dropEffect = event.dataTransfer.types.includes("Files") ? "copy" : "move";
      }}
      onDragLeave={(event) => {
        event.preventDefault();
        dragDepthRef.current = Math.max(0, dragDepthRef.current - 1);
        if (dragDepthRef.current === 0) setDragging(false);
      }}
      onDrop={(event) => {
        event.preventDefault();
        if (trashMode) return;
        dragDepthRef.current = 0;
        setDragging(false);
        const source = event.dataTransfer.getData(WORKSPACE_DRAG_TYPE);
        if (source) {
          void handleInternalMove(source, currentPath);
          return;
        }
        void startUploads(Array.from(event.dataTransfer.files));
      }}
      className="fixed inset-y-0 right-0 z-40 flex w-[min(760px,calc(100vw-1rem))] min-w-0 flex-col border-l border-border bg-background shadow-[-18px_0_60px_rgba(15,23,42,0.12)] xl:relative xl:inset-auto xl:z-auto xl:h-screen xl:w-[680px] xl:shrink-0 xl:shadow-none"
    >
      {dragging && !trashMode && (
        <div className="absolute inset-3 z-30 flex items-center justify-center rounded-2xl border-2 border-dashed border-cyan-500 bg-background/90 backdrop-blur-sm">
          <div className="text-center">
            <span className="mx-auto flex size-14 items-center justify-center rounded-2xl bg-cyan-500/10 text-cyan-600">
              <Upload className="size-7" />
            </span>
            <p className="mt-4 text-sm font-semibold">拖放到当前目录</p>
            <p className="mt-1 max-w-xs text-xs text-muted-foreground">
              文件将上传到 {currentPath || "我的文件"}，同名文件会先请求确认。
            </p>
          </div>
        </div>
      )}
      <header className="flex h-14 shrink-0 items-center gap-3 border-b border-border px-4">
        <span className="flex size-8 items-center justify-center rounded-lg bg-foreground text-background">
          {trashMode ? <Trash2 className="size-4" /> : <FolderOpen className="size-4" />}
        </span>
        <div className="min-w-0">
          <h2 className="text-sm font-semibold">{trashMode ? "回收站" : "会话工作区"}</h2>
          <p className="text-[11px] text-muted-foreground">
            {trashMode ? "已删除项目保留 30 天" : "浏览和预览智能体使用的文件"}
          </p>
        </div>
        <button
          type="button"
          onClick={() => void (trashMode ? loadTrash() : load())}
          disabled={loading}
          className="ml-auto rounded-md p-2 hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
          title="刷新文件"
          aria-label="刷新工作区文件"
        >
          <RefreshCw className={`size-4 text-muted-foreground ${loading ? "animate-spin" : ""}`} />
        </button>
        <button
          type="button"
          onClick={onClose}
          className="rounded-md p-2 hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
          title="关闭工作区"
          aria-label="关闭工作区"
        >
          <X className="size-4 text-muted-foreground" />
        </button>
      </header>

      <div className="grid min-h-0 flex-1 grid-cols-[176px_minmax(0,1fr)] max-sm:grid-cols-1">
        <nav className="min-h-0 overflow-y-auto border-r border-border bg-muted/20 p-2 max-sm:hidden" aria-label="工作区目录">
          <DirectoryTree
            directory={tree}
            currentPath={currentPath}
            expandedPaths={expandedPaths}
            onNavigate={navigate}
            onToggle={(path) =>
              setExpandedPaths((current) => {
                const next = new Set(current);
                if (next.has(path)) next.delete(path);
                else next.add(path);
                return next;
              })
            }
            onDropPath={(source, destination) => void handleInternalMove(source, destination)}
          />
          <div className="mt-2 border-t border-border pt-2">
            <button
              type="button"
              onClick={openTrash}
              className={`flex w-full items-center gap-2 rounded-md px-2 py-2 text-xs ${trashMode ? "bg-foreground text-background" : "text-muted-foreground hover:bg-muted hover:text-foreground"}`}
            >
              <Trash2 className="size-3.5" />
              回收站
              {trashItems.length > 0 && <span className="ml-auto text-[10px]">{trashItems.length}</span>}
            </button>
          </div>
        </nav>

        <main className="flex min-h-0 min-w-0 flex-col">
          {trashMode && (
            <div className="flex items-center gap-2 border-b border-border p-3">
              <button
                type="button"
                onClick={() => setTrashMode(false)}
                className="inline-flex h-9 items-center gap-1.5 rounded-lg border border-border px-3 text-xs font-medium hover:bg-muted"
              >
                <FolderOpen className="size-3.5" />
                返回我的文件
              </button>
              <p className="mr-auto text-[11px] text-muted-foreground">超过保留期限的项目会自动永久删除。</p>
              <button
                type="button"
                onClick={() => void handleEmptyTrash()}
                disabled={operationBusy || trashItems.length === 0}
                className="inline-flex h-9 items-center gap-1.5 rounded-lg border border-destructive/30 px-3 text-xs font-medium text-destructive hover:bg-destructive/10 disabled:opacity-40"
              >
                <Trash2 className="size-3.5" />
                清空回收站
              </button>
            </div>
          )}
          <div className={`${trashMode ? "hidden" : "grid"} gap-3 border-b border-border p-3`}>
            <div className="flex items-center gap-2">
              <div className="relative min-w-0 flex-1">
                <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
                <Input
                  value={query}
                  onChange={(event) => {
                    setQuery(event.target.value);
                    setPage(0);
                    setSelectedPaths(new Set());
                  }}
                  placeholder="搜索当前工作区"
                  aria-label="搜索工作区文件"
                  className="h-9 bg-muted/30 pl-8"
                />
              </div>
              <input
                ref={fileInputRef}
                type="file"
                multiple
                className="hidden"
                onChange={(event) => void startUploads(Array.from(event.target.files ?? []))}
              />
              <button
                type="button"
                onClick={() => {
                  setNewFolderName("");
                  setOperationError(null);
                  setNewFolderOpen(true);
                }}
                disabled={operationBusy}
                className="inline-flex h-9 shrink-0 items-center gap-1.5 rounded-lg border border-border px-3 text-xs font-medium hover:bg-muted disabled:opacity-50"
              >
                <FolderPlus className="size-3.5" />
                新建文件夹
              </button>
              <button
                type="button"
                onClick={() => fileInputRef.current?.click()}
                disabled={uploadBusy}
                className="inline-flex h-9 shrink-0 items-center gap-1.5 rounded-lg bg-foreground px-3 text-xs font-medium text-background transition-opacity hover:opacity-85 disabled:pointer-events-none disabled:opacity-50"
              >
                {uploadBusy ? <Loader2 className="size-3.5 animate-spin" /> : <Upload className="size-3.5" />}
                {uploadBusy ? "正在上传" : "上传文件"}
              </button>
            </div>

            <div className="flex min-w-0 items-center gap-1">
              {query.trim() ? (
                <div className="min-w-0 flex-1 truncate text-xs text-muted-foreground">
                  搜索结果 · 共 {totalFiles} 个文件
                </div>
              ) : (
                <div className="flex min-w-0 flex-1 items-center overflow-x-auto" aria-label="当前路径">
                  {breadcrumbs.map((breadcrumb, index) => (
                    <span key={breadcrumb.path || "root"} className="flex shrink-0 items-center">
                      {index > 0 && <ChevronRight className="size-3.5 text-muted-foreground/60" />}
                      <button
                        type="button"
                        onClick={() => navigate(breadcrumb.path)}
                        className={`rounded px-1.5 py-1 text-xs hover:bg-muted ${index === breadcrumbs.length - 1 ? "font-medium" : "text-muted-foreground"}`}
                      >
                        {breadcrumb.name}
                      </button>
                    </span>
                  ))}
                </div>
              )}
              <select
                value={sortBy}
                onChange={(event) => {
                  setSortBy(event.target.value as "name" | "size" | "updated");
                  setPage(0);
                }}
                aria-label="文件排序方式"
                className="h-8 shrink-0 rounded-lg border border-border bg-background px-2 text-[11px]"
              >
                <option value="name">按名称</option>
                <option value="updated">按更新时间</option>
                <option value="size">按大小</option>
              </select>
              <button
                type="button"
                onClick={() => {
                  setSortDirection((value) => value === "asc" ? "desc" : "asc");
                  setPage(0);
                }}
                className="h-8 shrink-0 rounded-lg border border-border px-2 text-[11px] hover:bg-muted"
                aria-label="切换排序方向"
              >
                {sortDirection === "asc" ? "升序" : "降序"}
              </button>
              <div className="flex shrink-0 rounded-lg border border-border p-0.5">
                <ViewButton active={view === "list"} label="列表视图" onClick={() => setView("list")}>
                  <List className="size-3.5" />
                </ViewButton>
                <ViewButton active={view === "grid"} label="网格视图" onClick={() => setView("grid")}>
                  <Grid2X2 className="size-3.5" />
                </ViewButton>
              </div>
            </div>
          </div>

          {error && (
            <div className="border-b border-border bg-destructive/10 px-4 py-2 text-xs text-destructive">
              {error}
            </div>
          )}

          {!trashMode && selectedPaths.size > 0 && (
            <div className="flex flex-wrap items-center gap-1.5 border-b border-border bg-cyan-500/5 px-3 py-2">
              <span className="mr-auto text-xs font-medium">已选择 {selectedPaths.size} 项</span>
              <SelectionAction label="插入对话" disabled={operationBusy || processingPaths} onClick={() => insertPathsIntoConversation([...selectedPaths])}>
                <MessageSquarePlus className="size-3.5" />
              </SelectionAction>
              <SelectionAction label="让智能体处理" disabled={operationBusy || processingPaths} onClick={() => void processWorkspacePaths([...selectedPaths])}>
                {processingPaths ? <Loader2 className="size-3.5 animate-spin" /> : <Sparkles className="size-3.5" />}
              </SelectionAction>
              {selectedPaths.size === 1 && (
                <SelectionAction label="重命名" disabled={operationBusy} onClick={openRename}>
                  <Pencil className="size-3.5" />
                </SelectionAction>
              )}
              <SelectionAction label="移动" disabled={operationBusy} onClick={() => openTransfer("move")}>
                <FolderInput className="size-3.5" />
              </SelectionAction>
              <SelectionAction label="复制" disabled={operationBusy} onClick={() => openTransfer("copy")}>
                <Copy className="size-3.5" />
              </SelectionAction>
              <SelectionAction label="移至回收站" disabled={operationBusy} destructive onClick={() => void handleBatchDelete()}>
                <Trash2 className="size-3.5" />
              </SelectionAction>
              <button
                type="button"
                onClick={() => setSelectedPaths(new Set())}
                className="rounded-md p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground"
                aria-label="取消选择"
              >
                <X className="size-3.5" />
              </button>
            </div>
          )}

          {trashMode && selectedTrashIds.size > 0 && (
            <div className="flex flex-wrap items-center gap-1.5 border-b border-border bg-cyan-500/5 px-3 py-2">
              <span className="mr-auto text-xs font-medium">已选择 {selectedTrashIds.size} 项</span>
              <SelectionAction label="还原" disabled={operationBusy} onClick={() => void handleRestoreTrash()}>
                <RotateCcw className="size-3.5" />
              </SelectionAction>
              <SelectionAction label="永久删除" disabled={operationBusy} destructive onClick={() => void handleDeleteTrash()}>
                <Trash2 className="size-3.5" />
              </SelectionAction>
              <button
                type="button"
                onClick={() => setSelectedTrashIds(new Set())}
                className="rounded-md p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground"
                aria-label="取消选择"
              >
                <X className="size-3.5" />
              </button>
            </div>
          )}

          <div className="min-h-0 flex-1 overflow-y-auto p-3">
            {trashMode ? (
              loading && trashItems.length === 0 ? (
                <WorkspaceEmpty icon={<Loader2 className="size-6 animate-spin" />} title="正在读取回收站" detail="正在加载可恢复的文件和目录。" />
              ) : trashItems.length > 0 ? (
                <WorkspaceTrashCollection
                  items={trashItems}
                  selectedIds={selectedTrashIds}
                  onToggle={(id) => setSelectedTrashIds((current) => togglePathSelection(current, id))}
                  onToggleAll={(ids) => setSelectedTrashIds((current) => toggleAllPaths(current, ids))}
                />
              ) : (
                <WorkspaceEmpty icon={<Trash2 className="size-6" />} title="回收站是空的" detail="移至回收站的文件和目录会在这里保留 30 天。" />
              )
            ) : loading && files.length === 0 ? (
              <WorkspaceEmpty icon={<Loader2 className="size-6 animate-spin" />} title="正在读取工作区" detail="正在加载文件和目录结构。" />
            ) : query.trim() ? (
              searchResults.length > 0 ? (
                <FileCollection
                  view={view}
                  entries={searchResults.map((file) => ({ kind: "file", name: workspaceFileName(file.path), file }))}
                  showFullPath
                  busyPath={busyPath}
                  selectedPaths={selectedPaths}
                  onToggleSelection={(path) =>
                    setSelectedPaths((current) => togglePathSelection(current, path))
                  }
                  onToggleAll={(paths) => setSelectedPaths((current) => toggleAllPaths(current, paths))}
                  onOpenFolder={navigate}
                  onPreview={setPreviewFile}
                  onDownload={handleDownload}
                  onDelete={handleDelete}
                  onMovePath={(source, destination) => void handleInternalMove(source, destination)}
                />
              ) : (
                <WorkspaceEmpty icon={<Search className="size-6" />} title="没有匹配的文件" detail="尝试使用文件名或目录名的其他关键词。" />
              )
            ) : entries.length > 0 ? (
              <FileCollection
                view={view}
                entries={entries}
                busyPath={busyPath}
                selectedPaths={selectedPaths}
                onToggleSelection={(path) =>
                  setSelectedPaths((current) => togglePathSelection(current, path))
                }
                onToggleAll={(paths) => setSelectedPaths((current) => toggleAllPaths(current, paths))}
                onOpenFolder={navigate}
                onPreview={setPreviewFile}
                onDownload={handleDownload}
                onDelete={handleDelete}
                onMovePath={(source, destination) => void handleInternalMove(source, destination)}
              />
            ) : (
              <WorkspaceEmpty
                icon={<FolderOpen className="size-6" />}
                title={files.length === 0 ? "工作区还是空的" : "这个目录是空的"}
                detail="上传文档、代码或图片，智能体即可在当前会话中使用。"
              />
            )}
          </div>

          {!trashMode && uploadItems.length > 0 && (
            <UploadQueue
              items={uploadItems}
              onCancel={cancelUpload}
              onRetry={(item) => void retryUpload(item)}
              onClear={() =>
                setUploadItems((current) =>
                  current.filter((item) => item.status === "queued" || item.status === "uploading"),
                )
              }
            />
          )}

          {trashMode ? (
            <footer className="flex items-center gap-3 border-t border-border px-4 py-2 text-[11px] text-muted-foreground">
              <span>共 {trashItems.length} 项</span>
              <span>占用 {formatBytes(trashItems.reduce((total, item) => total + item.size_bytes, 0))}</span>
              <span className="ml-auto">到期项目会在打开回收站时自动清理</span>
            </footer>
          ) : <footer className="flex items-center gap-3 border-t border-border px-4 py-2 text-[11px] text-muted-foreground">
            <span>第 {page + 1} 页 · 共 {totalFiles} 个文件</span>
            <span className="mr-auto">本页 {formatBytes(files.reduce((total, file) => total + file.size_bytes, 0))}</span>
            <button
              type="button"
              onClick={() => setPage((value) => Math.max(0, value - 1))}
              disabled={page === 0 || loading}
              className="rounded-md border border-border px-2 py-1 hover:bg-muted disabled:opacity-40"
            >
              上一页
            </button>
            <button
              type="button"
              onClick={() => setPage((value) => value + 1)}
              disabled={!hasNextPage || loading}
              className="rounded-md border border-border px-2 py-1 hover:bg-muted disabled:opacity-40"
            >
              下一页
            </button>
          </footer>}
        </main>
      </div>

      <WorkspacePreview
        sessionId={sessionId}
        file={previewFile}
        onClose={() => setPreviewFile(null)}
        onDownload={handleDownload}
        processing={processingPaths}
        onInsert={() => previewFile && insertPathsIntoConversation([previewFile.path])}
        onProcess={() => previewFile ? processWorkspacePaths([previewFile.path]) : Promise.resolve()}
      />
      <WorkspaceRenameDialog
        source={renameSource}
        name={renameName}
        busy={operationBusy}
        error={operationError}
        onNameChange={setRenameName}
        onClose={() => setRenameSource(null)}
        onSubmit={() => void commitRename()}
      />
      <WorkspaceTransferDialog
        mode={transferMode}
        count={selectedPaths.size}
        tree={tree}
        destination={transferDestination}
        busy={operationBusy}
        error={operationError}
        onDestinationChange={setTransferDestination}
        onClose={() => setTransferMode(null)}
        onSubmit={() => void commitTransfer()}
        status={operationStatus}
      />
      <WorkspaceNewFolderDialog
        open={newFolderOpen}
        name={newFolderName}
        currentPath={currentPath}
        busy={operationBusy}
        error={operationError}
        onNameChange={setNewFolderName}
        onClose={() => setNewFolderOpen(false)}
        onSubmit={() => void handleCreateFolder()}
      />
    </aside>
  );
}

function DirectoryTree({
  directory,
  currentPath,
  expandedPaths,
  onNavigate,
  onToggle,
  onDropPath,
  depth = 0,
}: {
  directory: WorkspaceDirectory;
  currentPath: string;
  expandedPaths: Set<string>;
  onNavigate: (path: string) => void;
  onToggle: (path: string) => void;
  onDropPath: (source: string, destination: string) => void;
  depth?: number;
}) {
  const expanded = expandedPaths.has(directory.path);
  const hasChildren = directory.children.length > 0;
  return (
    <div>
      <div
        onDragOver={(event) => {
          if (!event.dataTransfer.types.includes(WORKSPACE_DRAG_TYPE)) return;
          event.preventDefault();
          event.stopPropagation();
          event.dataTransfer.dropEffect = "move";
        }}
        onDrop={(event) => {
          const source = event.dataTransfer.getData(WORKSPACE_DRAG_TYPE);
          if (!source) return;
          event.preventDefault();
          event.stopPropagation();
          onDropPath(source, directory.path);
        }}
        className={`group flex items-center rounded-md ${currentPath === directory.path ? "bg-foreground text-background" : "hover:bg-muted"}`}
        style={{ paddingLeft: `${Math.min(depth, 5) * 10}px` }}
      >
        <button
          type="button"
          onClick={() => hasChildren && onToggle(directory.path)}
          disabled={!hasChildren}
          aria-label={`${expanded ? "折叠" : "展开"}${directory.name}`}
          className="flex size-6 shrink-0 items-center justify-center disabled:opacity-30"
        >
          {expanded ? <ChevronDown className="size-3" /> : <ChevronRight className="size-3" />}
        </button>
        <button
          type="button"
          onClick={() => onNavigate(directory.path)}
          title={directory.path || "我的文件"}
          className="flex min-w-0 flex-1 items-center gap-1.5 py-1.5 pr-2 text-left text-xs"
        >
          {expanded ? <FolderOpen className="size-3.5 shrink-0" /> : <Folder className="size-3.5 shrink-0" />}
          <span className="truncate">{directory.name}</span>
          <span className={`ml-auto text-[10px] ${currentPath === directory.path ? "text-background/65" : "text-muted-foreground"}`}>
            {directory.totalFileCount}
          </span>
        </button>
      </div>
      {expanded && directory.children.map((child) => (
        <DirectoryTree
          key={child.path}
          directory={child}
          currentPath={currentPath}
          expandedPaths={expandedPaths}
          onNavigate={onNavigate}
          onToggle={onToggle}
          onDropPath={onDropPath}
          depth={depth + 1}
        />
      ))}
    </div>
  );
}

function FileCollection({
  view,
  entries,
  showFullPath = false,
  busyPath,
  selectedPaths,
  onToggleSelection,
  onToggleAll,
  onOpenFolder,
  onPreview,
  onDownload,
  onDelete,
  onMovePath,
}: {
  view: "list" | "grid";
  entries: WorkspaceEntry[];
  showFullPath?: boolean;
  busyPath: string | null;
  selectedPaths: Set<string>;
  onToggleSelection: (path: string) => void;
  onToggleAll: (paths: string[]) => void;
  onOpenFolder: (path: string) => void;
  onPreview: (file: WorkspaceFile) => void;
  onDownload: (path: string) => Promise<void>;
  onDelete: (path: string) => Promise<void>;
  onMovePath: (source: string, destination: string) => void;
}) {
  const paths = entries.map((entry) => entry.kind === "folder" ? entry.path : entry.file.path);
  const allSelected = paths.length > 0 && paths.every((path) => selectedPaths.has(path));
  if (view === "grid") {
    return (
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
        {entries.map((entry) => {
          const path = entry.kind === "folder" ? entry.path : entry.file.path;
          return (
            <div
              key={entry.kind === "folder" ? `folder:${entry.path}` : entry.file.path}
              draggable
              onDragStart={(event) => {
                event.dataTransfer.setData(WORKSPACE_DRAG_TYPE, path);
                event.dataTransfer.effectAllowed = "move";
              }}
              onDragOver={(event) => {
                if (entry.kind !== "folder" || !event.dataTransfer.types.includes(WORKSPACE_DRAG_TYPE)) return;
                event.preventDefault();
                event.stopPropagation();
                event.dataTransfer.dropEffect = "move";
              }}
              onDrop={(event) => {
                if (entry.kind !== "folder") return;
                const source = event.dataTransfer.getData(WORKSPACE_DRAG_TYPE);
                if (!source) return;
                event.preventDefault();
                event.stopPropagation();
                onMovePath(source, entry.path);
              }}
              className={`relative min-w-0 rounded-xl border bg-card transition hover:border-foreground/25 hover:bg-muted/40 ${selectedPaths.has(path) ? "border-cyan-500 ring-2 ring-cyan-500/15" : "border-border"}`}
            >
              <input
                type="checkbox"
                checked={selectedPaths.has(path)}
                onChange={() => onToggleSelection(path)}
                aria-label={`选择 ${entry.name}`}
                className="absolute left-2.5 top-2.5 z-10 size-4 accent-cyan-600"
              />
              <button
                type="button"
                onClick={() => entry.kind === "folder" ? onOpenFolder(entry.path) : onPreview(entry.file)}
                className="w-full p-3 text-left"
              >
                <div className="flex h-20 items-center justify-center rounded-lg bg-muted/45">
                  {entry.kind === "folder" ? (
                    <Folder className="size-9 fill-amber-400/25 text-amber-600" />
                  ) : (
                    <WorkspaceFileIcon path={entry.file.path} className="size-9" />
                  )}
                </div>
                <p className="mt-2 truncate text-xs font-medium" title={path}>{entry.name}</p>
                <p className="mt-1 truncate text-[10px] text-muted-foreground">
                  {entry.kind === "folder" ? `${entry.fileCount} 个文件` : showFullPath ? parentWorkspacePath(entry.file.path) || "我的文件" : formatBytes(entry.file.size_bytes)}
                </p>
              </button>
            </div>
          );
        })}
      </div>
    );
  }

  return (
    <div className="overflow-hidden rounded-xl border border-border">
      <div className="grid grid-cols-[24px_minmax(0,1fr)_88px_112px_64px] gap-2 bg-muted/35 px-3 py-2 text-[10px] font-medium text-muted-foreground max-sm:grid-cols-[24px_minmax(0,1fr)_64px]">
        <input
          type="checkbox"
          checked={allSelected}
          onChange={() => onToggleAll(paths)}
          aria-label="选择当前全部项目"
          className="size-3.5 accent-cyan-600"
        />
        <span>名称</span>
        <span className="max-sm:hidden">大小</span>
        <span className="max-sm:hidden">更新时间</span>
        <span className="text-right">操作</span>
      </div>
      {entries.map((entry) => {
        const path = entry.kind === "folder" ? entry.path : entry.file.path;
        return (
          <div
            key={entry.kind === "folder" ? `folder:${entry.path}` : entry.file.path}
            draggable
            onDragStart={(event) => {
              event.dataTransfer.setData(WORKSPACE_DRAG_TYPE, path);
              event.dataTransfer.effectAllowed = "move";
            }}
            onDragOver={(event) => {
              if (entry.kind !== "folder" || !event.dataTransfer.types.includes(WORKSPACE_DRAG_TYPE)) return;
              event.preventDefault();
              event.stopPropagation();
              event.dataTransfer.dropEffect = "move";
            }}
            onDrop={(event) => {
              if (entry.kind !== "folder") return;
              const source = event.dataTransfer.getData(WORKSPACE_DRAG_TYPE);
              if (!source) return;
              event.preventDefault();
              event.stopPropagation();
              onMovePath(source, entry.path);
            }}
            className={`grid grid-cols-[24px_minmax(0,1fr)_88px_112px_64px] items-center gap-2 border-t border-border px-3 py-2.5 transition hover:bg-muted/30 max-sm:grid-cols-[24px_minmax(0,1fr)_64px] ${selectedPaths.has(path) ? "bg-cyan-500/5" : ""}`}
          >
            <input
              type="checkbox"
              checked={selectedPaths.has(path)}
              onChange={() => onToggleSelection(path)}
              aria-label={`选择 ${entry.name}`}
              className="size-3.5 accent-cyan-600"
            />
            <button
              type="button"
              onClick={() => entry.kind === "folder" ? onOpenFolder(entry.path) : onPreview(entry.file)}
              className="flex min-w-0 items-center gap-2 text-left"
            >
              {entry.kind === "folder" ? (
                <Folder className="size-4 shrink-0 fill-amber-400/25 text-amber-600" />
              ) : (
                <WorkspaceFileIcon path={entry.file.path} className="size-4 shrink-0" />
              )}
              <span className="min-w-0">
                <span className="block truncate text-xs font-medium" title={path}>{entry.name}</span>
                {showFullPath && entry.kind === "file" && (
                  <span className="block truncate text-[10px] text-muted-foreground">{parentWorkspacePath(entry.file.path) || "我的文件"}</span>
                )}
              </span>
            </button>
            <span className="text-[11px] text-muted-foreground max-sm:hidden">
              {entry.kind === "folder" ? `${entry.fileCount} 项` : formatBytes(entry.file.size_bytes)}
            </span>
            <span className="truncate text-[11px] text-muted-foreground max-sm:hidden">
              {entry.kind === "folder" ? "—" : formatDate(entry.file.updated_at)}
            </span>
            <span className="flex justify-end gap-0.5">
              {entry.kind === "file" && (
                <>
                  <IconAction
                    label={`下载 ${entry.name}`}
                    disabled={busyPath === path}
                    onClick={() => void onDownload(path)}
                  >
                    <Download className="size-3.5" />
                  </IconAction>
                  <IconAction
                    label={`删除 ${entry.name}`}
                    disabled={busyPath === path}
                    destructive
                    onClick={() => void onDelete(path)}
                  >
                    <Trash2 className="size-3.5" />
                  </IconAction>
                </>
              )}
            </span>
          </div>
        );
      })}
    </div>
  );
}

function WorkspaceTrashCollection({
  items,
  selectedIds,
  onToggle,
  onToggleAll,
}: {
  items: WorkspaceTrashItem[];
  selectedIds: Set<string>;
  onToggle: (id: string) => void;
  onToggleAll: (ids: string[]) => void;
}) {
  const ids = items.map((item) => item.id);
  const allSelected = ids.every((id) => selectedIds.has(id));
  return (
    <div className="overflow-hidden rounded-xl border border-border">
      <div className="grid grid-cols-[28px_minmax(0,1fr)_90px_116px] items-center gap-2 border-b border-border bg-muted/30 px-3 py-2 text-[10px] font-medium text-muted-foreground">
        <input
          type="checkbox"
          checked={allSelected}
          onChange={() => onToggleAll(ids)}
          aria-label={allSelected ? "取消全选回收站项目" : "全选回收站项目"}
          className="size-4 accent-cyan-600"
        />
        <span>原位置</span>
        <span>大小</span>
        <span>保留期限</span>
      </div>
      {items.map((item) => {
        const primaryPath = item.paths[0] ?? "未知位置";
        return (
          <label
            key={item.id}
            className={`grid cursor-pointer grid-cols-[28px_minmax(0,1fr)_90px_116px] items-center gap-2 border-b border-border/70 px-3 py-3 last:border-b-0 hover:bg-muted/30 ${selectedIds.has(item.id) ? "bg-cyan-500/5" : ""}`}
          >
            <input
              type="checkbox"
              checked={selectedIds.has(item.id)}
              onChange={() => onToggle(item.id)}
              aria-label={`选择 ${primaryPath}`}
              className="size-4 accent-cyan-600"
            />
            <span className="flex min-w-0 items-center gap-2">
              <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted text-muted-foreground">
                <Trash2 className="size-4" />
              </span>
              <span className="min-w-0">
                <span className="block truncate text-xs font-medium" title={item.paths.join("、")}>
                  {primaryPath}{item.paths.length > 1 ? ` 等 ${item.paths.length} 个位置` : ""}
                </span>
                <span className="block text-[10px] text-muted-foreground">
                  {formatDate(item.deleted_at)}删除 · {item.object_count} 个对象
                </span>
              </span>
            </span>
            <span className="text-[11px] text-muted-foreground">{formatBytes(item.size_bytes)}</span>
            <span className="text-[11px] text-muted-foreground">
              保留至 {new Date(item.expires_at).toLocaleDateString("zh-CN")}
            </span>
          </label>
        );
      })}
    </div>
  );
}

function WorkspaceRenameDialog({
  source,
  name,
  busy,
  error,
  onNameChange,
  onClose,
  onSubmit,
}: {
  source: string | null;
  name: string;
  busy: boolean;
  error: string | null;
  onNameChange: (name: string) => void;
  onClose: () => void;
  onSubmit: () => void;
}) {
  return (
    <Dialog open={Boolean(source)} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>重命名文件或目录</DialogTitle>
          <DialogDescription className="truncate" title={source ?? undefined}>
            {source}
          </DialogDescription>
        </DialogHeader>
        <form
          className="grid gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            onSubmit();
          }}
        >
          <Input
            autoFocus
            value={name}
            onChange={(event) => onNameChange(event.target.value)}
            aria-label="新名称"
            placeholder="输入新名称"
          />
          {error && <p className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{error}</p>}
          <div className="flex justify-end gap-2">
            <button type="button" onClick={onClose} disabled={busy} className="h-8 rounded-lg border border-border px-3 text-xs hover:bg-muted disabled:opacity-50">
              取消
            </button>
            <button type="submit" disabled={busy || !name.trim()} className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-foreground px-3 text-xs font-medium text-background disabled:opacity-50">
              {busy && <Loader2 className="size-3.5 animate-spin" />}
              确认重命名
            </button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function WorkspaceTransferDialog({
  mode,
  count,
  tree,
  destination,
  busy,
  error,
  status,
  onDestinationChange,
  onClose,
  onSubmit,
}: {
  mode: "move" | "copy" | null;
  count: number;
  tree: WorkspaceDirectory;
  destination: string;
  busy: boolean;
  error: string | null;
  status: string | null;
  onDestinationChange: (path: string) => void;
  onClose: () => void;
  onSubmit: () => void;
}) {
  return (
    <Dialog open={Boolean(mode)} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="flex max-h-[70vh] max-w-lg grid-rows-none flex-col overflow-hidden">
        <DialogHeader>
          <DialogTitle>{mode === "move" ? "移动到" : "复制到"}</DialogTitle>
          <DialogDescription>
            已选择 {count} 项，请选择目标目录。文件操作在对象存储服务端完成。
          </DialogDescription>
        </DialogHeader>
        <div className="min-h-48 flex-1 overflow-y-auto rounded-lg border border-border bg-muted/20 p-2">
          <FolderPicker
            directory={tree}
            selectedPath={destination}
            onSelect={onDestinationChange}
          />
        </div>
        <div className="rounded-lg bg-muted/40 px-3 py-2 text-xs text-muted-foreground">
          目标位置：{destination || "我的文件"}
        </div>
        {busy && status && (
          <p className="flex items-center gap-2 rounded-lg bg-cyan-500/10 px-3 py-2 text-xs text-cyan-700 dark:text-cyan-300">
            <Loader2 className="size-3.5 animate-spin" />
            {status}
          </p>
        )}
        {error && <p className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{error}</p>}
        <div className="flex justify-end gap-2">
          <button type="button" onClick={onClose} disabled={busy} className="h-8 rounded-lg border border-border px-3 text-xs hover:bg-muted disabled:opacity-50">
            取消
          </button>
          <button type="button" onClick={onSubmit} disabled={busy} className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-foreground px-3 text-xs font-medium text-background disabled:opacity-50">
            {busy && <Loader2 className="size-3.5 animate-spin" />}
            {mode === "move" ? "移动到这里" : "复制到这里"}
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function WorkspaceNewFolderDialog({
  open,
  name,
  currentPath,
  busy,
  error,
  onNameChange,
  onClose,
  onSubmit,
}: {
  open: boolean;
  name: string;
  currentPath: string;
  busy: boolean;
  error: string | null;
  onNameChange: (name: string) => void;
  onClose: () => void;
  onSubmit: () => void;
}) {
  return (
    <Dialog open={open} onOpenChange={(nextOpen) => !nextOpen && onClose()}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>新建文件夹</DialogTitle>
          <DialogDescription>
            创建位置：{currentPath || "我的文件"}
          </DialogDescription>
        </DialogHeader>
        <form
          className="grid gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            onSubmit();
          }}
        >
          <Input
            autoFocus
            value={name}
            onChange={(event) => onNameChange(event.target.value)}
            aria-label="文件夹名称"
            placeholder="输入文件夹名称"
          />
          {error && <p className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{error}</p>}
          <div className="flex justify-end gap-2">
            <button type="button" onClick={onClose} disabled={busy} className="h-8 rounded-lg border border-border px-3 text-xs hover:bg-muted disabled:opacity-50">
              取消
            </button>
            <button type="submit" disabled={busy || !name.trim()} className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-foreground px-3 text-xs font-medium text-background disabled:opacity-50">
              {busy && <Loader2 className="size-3.5 animate-spin" />}
              创建文件夹
            </button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function FolderPicker({
  directory,
  selectedPath,
  onSelect,
  depth = 0,
}: {
  directory: WorkspaceDirectory;
  selectedPath: string;
  onSelect: (path: string) => void;
  depth?: number;
}) {
  return (
    <div>
      <button
        type="button"
        onClick={() => onSelect(directory.path)}
        style={{ paddingLeft: `${10 + depth * 16}px` }}
        className={`flex w-full items-center gap-2 rounded-md py-2 pr-3 text-left text-xs ${selectedPath === directory.path ? "bg-foreground text-background" : "hover:bg-muted"}`}
      >
        <Folder className="size-4 shrink-0" />
        <span className="min-w-0 flex-1 truncate">{directory.name}</span>
        <span className={`text-[10px] ${selectedPath === directory.path ? "text-background/65" : "text-muted-foreground"}`}>{directory.totalFileCount}</span>
      </button>
      {directory.children.map((child) => (
        <FolderPicker
          key={child.path}
          directory={child}
          selectedPath={selectedPath}
          onSelect={onSelect}
          depth={depth + 1}
        />
      ))}
    </div>
  );
}

function SelectionAction({
  label,
  disabled,
  destructive = false,
  onClick,
  children,
}: {
  label: string;
  disabled: boolean;
  destructive?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className={`inline-flex h-7 items-center gap-1 rounded-md border px-2 text-[11px] disabled:opacity-50 ${destructive ? "border-destructive/25 text-destructive hover:bg-destructive/10" : "border-border bg-background hover:bg-muted"}`}
    >
      {children}
      {label}
    </button>
  );
}

function togglePathSelection(current: Set<string>, path: string): Set<string> {
  const next = new Set(current);
  if (next.has(path)) next.delete(path);
  else next.add(path);
  return next;
}

function toggleAllPaths(current: Set<string>, paths: string[]): Set<string> {
  const next = new Set(current);
  const allSelected = paths.length > 0 && paths.every((path) => next.has(path));
  paths.forEach((path) => allSelected ? next.delete(path) : next.add(path));
  return next;
}

function UploadQueue({
  items,
  onCancel,
  onRetry,
  onClear,
}: {
  items: WorkspaceUploadItem[];
  onCancel: (id: string) => void;
  onRetry: (item: WorkspaceUploadItem) => void;
  onClear: () => void;
}) {
  const active = items.some((item) => item.status === "queued" || item.status === "uploading");
  return (
    <section className="max-h-52 shrink-0 overflow-y-auto border-t border-border bg-background shadow-[0_-10px_30px_rgba(15,23,42,0.06)]" aria-label="上传队列">
      <div className="sticky top-0 z-10 flex items-center justify-between border-b border-border bg-background/95 px-3 py-2 backdrop-blur">
        <div>
          <h3 className="text-xs font-semibold">上传队列</h3>
          <p className="text-[10px] text-muted-foreground">
            {active ? "正在按顺序上传文件" : "本批次处理完成"}
          </p>
        </div>
        <button
          type="button"
          onClick={onClear}
          className="rounded-md px-2 py-1 text-[10px] text-muted-foreground hover:bg-muted hover:text-foreground"
        >
          清理完成项
        </button>
      </div>
      <div className="divide-y divide-border">
        {items.map((item) => (
          <div key={item.id} className="grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-2 px-3 py-2">
            <UploadStatusIcon status={item.status} />
            <div className="min-w-0">
              <div className="flex items-center justify-between gap-2">
                <p className="truncate text-[11px] font-medium" title={item.path}>{workspaceFileName(item.path)}</p>
                <span className="shrink-0 text-[10px] text-muted-foreground">{uploadStatusLabel(item)}</span>
              </div>
              <p className="mt-0.5 truncate text-[10px] text-muted-foreground" title={item.error ?? item.path}>
                {item.error ?? `${parentWorkspacePath(item.path) || "我的文件"} · ${formatBytes(item.file.size)}`}
              </p>
              {(item.status === "uploading" || item.status === "queued") && (
                <div className="mt-1.5 h-1 overflow-hidden rounded-full bg-muted">
                  <div
                    className={`h-full rounded-full bg-cyan-500 transition-[width] ${item.status === "queued" ? "opacity-30" : ""}`}
                    style={{ width: `${item.status === "queued" ? 2 : Math.max(2, item.progress)}%` }}
                  />
                </div>
              )}
            </div>
            {item.status === "queued" || item.status === "uploading" ? (
              <IconAction label={`取消上传 ${item.path}`} disabled={false} onClick={() => onCancel(item.id)}>
                <CircleX className="size-3.5" />
              </IconAction>
            ) : item.status === "error" || item.status === "cancelled" ? (
              <IconAction label={`重试上传 ${item.path}`} disabled={active} onClick={() => onRetry(item)}>
                <RotateCcw className="size-3.5" />
              </IconAction>
            ) : (
              <span className="size-6" />
            )}
          </div>
        ))}
      </div>
    </section>
  );
}

function UploadStatusIcon({ status }: { status: UploadStatus }) {
  if (status === "success") return <CheckCircle2 className="size-4 text-emerald-600" />;
  if (status === "error") return <CircleAlert className="size-4 text-destructive" />;
  if (status === "cancelled" || status === "skipped") return <CircleX className="size-4 text-muted-foreground" />;
  if (status === "uploading") return <Loader2 className="size-4 animate-spin text-cyan-600" />;
  return <Upload className="size-4 text-muted-foreground" />;
}

function uploadStatusLabel(item: WorkspaceUploadItem): string {
  if (item.status === "queued") return "等待上传";
  if (item.status === "uploading") return `${item.progress}%`;
  if (item.status === "success") return "上传完成";
  if (item.status === "error") return "上传失败";
  if (item.status === "cancelled") return "已取消";
  return "同名已跳过";
}

function WorkspacePreview({
  sessionId,
  file,
  onClose,
  onDownload,
  processing,
  onInsert,
  onProcess,
}: {
  sessionId: string;
  file: WorkspaceFile | null;
  onClose: () => void;
  onDownload: (path: string) => Promise<void>;
  processing: boolean;
  onInsert: () => void;
  onProcess: () => Promise<void>;
}) {
  const [url, setUrl] = useState<string | null>(null);
  const [text, setText] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const kind = file ? workspacePreviewKind(file.path) : "unsupported";

  useEffect(() => {
    if (!file) return;
    let cancelled = false;
    setUrl(null);
    setText(null);
    setError(null);
    setLoading(true);
    if (["text", "code", "json", "csv"].includes(kind) && file.size_bytes > MAX_TEXT_PREVIEW_BYTES) {
      setError("该文件超过 2 MB，为避免浏览器卡顿，请下载后查看。 ");
      setLoading(false);
      return;
    }
    workspaceFileDownloadUrl(sessionId, file.path)
      .then(async (nextUrl) => {
        if (cancelled) return;
        setUrl(nextUrl);
        if (["text", "code", "json", "csv"].includes(kind)) {
          const response = await fetch(nextUrl);
          if (!response.ok) throw new Error(`读取预览失败：HTTP ${response.status}`);
          const content = await response.text();
          if (!cancelled) setText(content);
        }
      })
      .catch((reason) => {
        if (!cancelled) setError(reason instanceof Error ? reason.message : String(reason));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [file, kind, sessionId]);

  return (
    <Dialog open={Boolean(file)} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="flex h-[min(86vh,820px)] max-w-[min(1100px,calc(100vw-2rem))] grid-rows-none flex-col gap-0 overflow-hidden p-0">
        {file && (
          <>
            <DialogHeader className="border-b border-border px-5 py-4 pr-14">
              <div className="flex items-center gap-3">
                <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-muted">
                  <WorkspaceFileIcon path={file.path} className="size-5" />
                </span>
                <div className="min-w-0 flex-1">
                  <DialogTitle className="truncate" title={file.path}>{workspaceFileName(file.path)}</DialogTitle>
                  <DialogDescription className="mt-1 truncate">
                    {file.path} · {formatBytes(file.size_bytes)} · {file.content_type ?? "未知格式"} · {formatDate(file.updated_at)}
                  </DialogDescription>
                  {file.etag && (
                    <p className="mt-1 truncate font-mono text-[10px] text-muted-foreground" title={file.etag}>
                      ETag：{file.etag}
                    </p>
                  )}
                </div>
                <button
                  type="button"
                  onClick={onInsert}
                  className="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-lg border border-border px-3 text-xs font-medium hover:bg-muted"
                >
                  <MessageSquarePlus className="size-3.5" />
                  插入对话
                </button>
                <button
                  type="button"
                  onClick={() => void onProcess()}
                  disabled={processing}
                  className="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-lg bg-foreground px-3 text-xs font-medium text-background hover:opacity-85 disabled:opacity-50"
                >
                  {processing ? <Loader2 className="size-3.5 animate-spin" /> : <Sparkles className="size-3.5" />}
                  让智能体处理
                </button>
                <button
                  type="button"
                  onClick={() => void onDownload(file.path)}
                  className="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-lg border border-border px-3 text-xs font-medium hover:bg-muted"
                >
                  <Download className="size-3.5" />
                  下载
                </button>
              </div>
            </DialogHeader>
            <div className="min-h-0 flex-1 overflow-auto bg-muted/25 p-4">
              {loading ? (
                <div className="flex h-full items-center justify-center gap-2 text-sm text-muted-foreground">
                  <Loader2 className="size-4 animate-spin" />
                  正在生成预览
                </div>
              ) : error ? (
                <PreviewMessage icon={<FileQuestion className="size-8" />} title="暂时无法预览" detail={error} />
              ) : kind === "image" && url ? (
                <div className="flex min-h-full items-center justify-center">
                  {/* eslint-disable-next-line @next/next/no-img-element */}
                  <img src={url} alt={workspaceFileName(file.path)} className="max-h-full max-w-full rounded-lg object-contain shadow-sm" />
                </div>
              ) : kind === "pdf" && url ? (
                <iframe src={url} title={workspaceFileName(file.path)} className="h-full min-h-[600px] w-full rounded-lg border border-border bg-white" />
              ) : kind === "csv" && text !== null ? (
                <CsvPreview content={text} separator={file.path.toLowerCase().endsWith(".tsv") ? "\t" : ","} />
              ) : (kind === "code" || kind === "json" || kind === "text") && text !== null ? (
                <HighlightedCode
                  code={kind === "json" ? prettyJson(text) : text}
                  lang={kind === "text" ? workspaceCodeLanguage(file.path) : kind === "json" ? "json" : workspaceCodeLanguage(file.path)}
                  className="min-h-full [&_pre]:min-h-full"
                />
              ) : (
                <PreviewMessage
                  icon={<FileQuestion className="size-8" />}
                  title="此格式不支持在线预览"
                  detail="为保证安全，HTML、SVG、压缩包、Office 和其他二进制文件需要下载后查看。"
                />
              )}
            </div>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}

function CsvPreview({ content, separator }: { content: string; separator: string }) {
  const rows = content.split(/\r?\n/).filter(Boolean).slice(0, 101).map((line) => line.split(separator));
  const headers = rows[0] ?? [];
  return (
    <div className="overflow-auto rounded-lg border border-border bg-background">
      <table className="w-full min-w-max text-left text-xs">
        <thead className="sticky top-0 bg-muted">
          <tr>{headers.map((header, index) => <th key={index} className="border-b border-r border-border px-3 py-2 font-medium">{header}</th>)}</tr>
        </thead>
        <tbody>
          {rows.slice(1).map((row, rowIndex) => (
            <tr key={rowIndex} className="border-b border-border/60">
              {headers.map((_, columnIndex) => <td key={columnIndex} className="border-r border-border/60 px-3 py-2">{row[columnIndex] ?? ""}</td>)}
            </tr>
          ))}
        </tbody>
      </table>
      {content.split(/\r?\n/).filter(Boolean).length > 101 && (
        <p className="p-3 text-center text-xs text-muted-foreground">仅显示前 100 行，请下载文件查看完整数据。</p>
      )}
    </div>
  );
}

function prettyJson(content: string): string {
  try {
    return JSON.stringify(JSON.parse(content), null, 2);
  } catch {
    return content;
  }
}

function WorkspaceFileIcon({ path, className }: { path: string; className?: string }) {
  const kind = workspacePreviewKind(path);
  if (kind === "image") return <FileImage className={`${className ?? ""} text-sky-600`} />;
  if (kind === "pdf") return <FileText className={`${className ?? ""} text-red-600`} />;
  if (kind === "json") return <FileJson className={`${className ?? ""} text-amber-600`} />;
  if (kind === "csv") return <FileSpreadsheet className={`${className ?? ""} text-emerald-600`} />;
  if (kind === "code") return <FileCode2 className={`${className ?? ""} text-cyan-600`} />;
  if (kind === "text") return <FileText className={`${className ?? ""} text-slate-600 dark:text-slate-300`} />;
  return <File className={`${className ?? ""} text-muted-foreground`} />;
}

function ViewButton({ active, label, onClick, children }: { active: boolean; label: string; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      aria-pressed={active}
      className={`flex size-7 items-center justify-center rounded-md ${active ? "bg-foreground text-background" : "text-muted-foreground hover:bg-muted"}`}
    >
      {children}
    </button>
  );
}

function IconAction({ label, disabled, destructive = false, onClick, children }: { label: string; disabled: boolean; destructive?: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={label}
      aria-label={label}
      className={`rounded-md p-1.5 disabled:pointer-events-none disabled:opacity-40 ${destructive ? "text-muted-foreground hover:bg-destructive/10 hover:text-destructive" : "text-muted-foreground hover:bg-muted hover:text-foreground"}`}
    >
      {children}
    </button>
  );
}

function WorkspaceEmpty({ icon, title, detail }: { icon: React.ReactNode; title: string; detail: string }) {
  return (
    <div className="flex min-h-56 flex-col items-center justify-center gap-2 p-8 text-center">
      <span className="flex size-12 items-center justify-center rounded-2xl bg-muted text-muted-foreground">{icon}</span>
      <p className="mt-2 text-sm font-medium">{title}</p>
      <p className="max-w-xs text-xs leading-5 text-muted-foreground">{detail}</p>
    </div>
  );
}

function PreviewMessage({ icon, title, detail }: { icon: React.ReactNode; title: string; detail: string }) {
  return (
    <div className="flex h-full min-h-80 flex-col items-center justify-center text-center">
      <span className="flex size-16 items-center justify-center rounded-2xl bg-background text-muted-foreground shadow-sm">{icon}</span>
      <h3 className="mt-4 text-sm font-semibold">{title}</h3>
      <p className="mt-2 max-w-md text-xs leading-5 text-muted-foreground">{detail}</p>
    </div>
  );
}
