import type { WorkspaceFile } from "@/lib/types";

export interface WorkspaceDirectory {
  name: string;
  path: string;
  children: WorkspaceDirectory[];
  directFileCount: number;
  totalFileCount: number;
}

export interface WorkspaceFolderEntry {
  kind: "folder";
  name: string;
  path: string;
  fileCount: number;
}

export interface WorkspaceFileEntry {
  kind: "file";
  name: string;
  file: WorkspaceFile;
}

export type WorkspaceEntry = WorkspaceFolderEntry | WorkspaceFileEntry;

export function parentWorkspacePath(path: string): string {
  const parts = path.split("/").filter(Boolean);
  return parts.slice(0, -1).join("/");
}

export function workspaceFileName(path: string): string {
  return path.split("/").filter(Boolean).at(-1) ?? path;
}

export function workspaceBreadcrumbs(path: string): Array<{ name: string; path: string }> {
  const parts = path.split("/").filter(Boolean);
  return [
    { name: "我的文件", path: "" },
    ...parts.map((name, index) => ({ name, path: parts.slice(0, index + 1).join("/") })),
  ];
}

export function buildWorkspaceTree(files: WorkspaceFile[]): WorkspaceDirectory {
  const root: WorkspaceDirectory = {
    name: "我的文件",
    path: "",
    children: [],
    directFileCount: 0,
    totalFileCount: files.length,
  };
  const directories = new Map<string, WorkspaceDirectory>([["", root]]);

  for (const file of files) {
    const segments = file.path.split("/").filter(Boolean);
    let parent = root;
    for (let index = 0; index < segments.length - 1; index += 1) {
      const path = segments.slice(0, index + 1).join("/");
      let directory = directories.get(path);
      if (!directory) {
        directory = {
          name: segments[index],
          path,
          children: [],
          directFileCount: 0,
          totalFileCount: 0,
        };
        directories.set(path, directory);
        parent.children.push(directory);
      }
      directory.totalFileCount += 1;
      parent = directory;
    }
    parent.directFileCount += 1;
  }

  for (const directory of directories.values()) {
    directory.children.sort((a, b) => a.name.localeCompare(b.name, "zh-CN"));
  }
  return root;
}

export function buildWorkspaceTreeFromFolders(
  folders: string[],
  files: WorkspaceFile[] = [],
): WorkspaceDirectory {
  const markers = folders.map((path) => ({
    path: `${path}/.folder-marker`,
    size_bytes: 0,
    updated_at: null,
  }));
  return buildWorkspaceTree([...files, ...markers]);
}

export function listWorkspaceEntries(
  files: WorkspaceFile[],
  currentPath: string,
  knownFolders: string[] = [],
): WorkspaceEntry[] {
  const prefix = currentPath ? `${currentPath}/` : "";
  const folders = new Map<string, WorkspaceFolderEntry>();
  const directFiles: WorkspaceFileEntry[] = [];

  for (const folderPath of knownFolders) {
    if (parentWorkspacePath(folderPath) !== currentPath) continue;
    folders.set(folderPath, {
      kind: "folder",
      name: workspaceFileName(folderPath),
      path: folderPath,
      fileCount: 0,
    });
  }

  for (const file of files) {
    if (!file.path.startsWith(prefix)) continue;
    const remainder = file.path.slice(prefix.length);
    if (!remainder || remainder.startsWith("/")) continue;
    const slash = remainder.indexOf("/");
    if (slash === -1) {
      directFiles.push({ kind: "file", name: workspaceFileName(file.path), file });
      continue;
    }
    const name = remainder.slice(0, slash);
    const path = prefix ? `${currentPath}/${name}` : name;
    const existing = folders.get(path);
    if (existing) {
      existing.fileCount += 1;
    } else {
      folders.set(path, { kind: "folder", name, path, fileCount: 1 });
    }
  }

  return [
    ...[...folders.values()].sort((a, b) => a.name.localeCompare(b.name, "zh-CN")),
    ...directFiles.sort((a, b) => a.name.localeCompare(b.name, "zh-CN")),
  ];
}

export function searchWorkspaceFiles(files: WorkspaceFile[], query: string): WorkspaceFile[] {
  const normalized = query.trim().toLocaleLowerCase("zh-CN");
  if (!normalized) return [];
  return files
    .filter((file) => file.path.toLocaleLowerCase("zh-CN").includes(normalized))
    .sort((a, b) => a.path.localeCompare(b.path, "zh-CN"));
}

export function workspaceUploadPath(currentPath: string, fileName: string): string {
  const normalizedName = fileName.trim().replace(/^\/+/, "");
  return currentPath ? `${currentPath}/${normalizedName}` : normalizedName;
}

export function conflictingWorkspacePaths(
  files: WorkspaceFile[],
  uploadPaths: string[],
): string[] {
  const existing = new Set(files.map((file) => file.path));
  return [...new Set(uploadPaths.filter((path) => existing.has(path)))].sort((a, b) =>
    a.localeCompare(b, "zh-CN"),
  );
}

export function workspaceTransferDestination(sourcePath: string, directoryPath: string): string {
  const name = workspaceFileName(sourcePath);
  return directoryPath ? `${directoryPath}/${name}` : name;
}

export function workspaceTransferConflicts(
  files: WorkspaceFile[],
  sourcePaths: string[],
  directoryPath: string,
): string[] {
  const existing = new Set(files.map((file) => file.path));
  const sourceObjects = new Set(
    files
      .filter((file) =>
        sourcePaths.some(
          (source) => file.path === source || file.path.startsWith(`${source}/`),
        ),
      )
      .map((file) => file.path),
  );
  const conflicts = new Set<string>();
  for (const source of sourcePaths) {
    const destination = workspaceTransferDestination(source, directoryPath);
    const exactFile = sourceObjects.has(source);
    if (exactFile) {
      if (existing.has(destination) && !sourceObjects.has(destination)) conflicts.add(destination);
      continue;
    }
    const prefix = `${source}/`;
    for (const file of files) {
      const suffix = file.path.startsWith(prefix) ? file.path.slice(prefix.length) : null;
      if (suffix === null) continue;
      const target = `${destination}/${suffix}`;
      if (existing.has(target) && !sourceObjects.has(target)) conflicts.add(target);
    }
  }
  return [...conflicts].sort((a, b) => a.localeCompare(b, "zh-CN"));
}

export type WorkspacePreviewKind = "image" | "pdf" | "text" | "code" | "json" | "csv" | "unsupported";

export function workspacePreviewKind(path: string): WorkspacePreviewKind {
  const extension = path.split(".").at(-1)?.toLowerCase() ?? "";
  if (["png", "jpg", "jpeg", "gif", "webp", "bmp", "avif"].includes(extension)) return "image";
  if (extension === "pdf") return "pdf";
  if (extension === "json") return "json";
  if (["csv", "tsv"].includes(extension)) return "csv";
  if (
    [
      "js", "jsx", "ts", "tsx", "py", "rs", "go", "java", "kt", "swift", "c", "h", "cpp",
      "css", "scss", "sql", "sh", "bash", "zsh", "yaml", "yml", "toml", "xml", "vue", "svelte",
    ].includes(extension)
  ) return "code";
  if (["txt", "md", "mdx", "log", "ini", "conf", "env"].includes(extension)) return "text";
  return "unsupported";
}

export function workspaceCodeLanguage(path: string): string {
  const extension = path.split(".").at(-1)?.toLowerCase() ?? "text";
  const aliases: Record<string, string> = {
    js: "javascript",
    jsx: "jsx",
    ts: "typescript",
    tsx: "tsx",
    py: "python",
    rs: "rust",
    sh: "bash",
    yml: "yaml",
  };
  return aliases[extension] ?? extension;
}

function workspaceReferenceList(paths: string[]): string {
  return [...new Set(paths)]
    .sort((a, b) => a.localeCompare(b, "zh-CN"))
    .map((path) => `- \`${path}\``)
    .join("\n");
}

export function workspaceConversationReference(paths: string[]): string {
  return `请参考以下会话工作区路径：\n${workspaceReferenceList(paths)}`;
}

export function workspaceAgentTaskPrompt(paths: string[]): string {
  return `请读取并处理以下会话工作区路径：\n${workspaceReferenceList(paths)}\n\n请根据文件内容完成分析或处理任务；如果目标不明确，先说明你对任务的理解并提出必要的澄清问题。完成后请列出使用过的文件和产生的新文件。`;
}
