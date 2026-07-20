"use client";

import { useEffect, useMemo, useState } from "react";
import {
  Wrench,
  Plus,
  Trash2,
  Pencil,
  Loader2,
  Search,
  Copy,
  Check,
  Sparkles,
  Terminal,
  Cpu,
  BookOpen,
  Edit3,
  Eye,
  Zap,
  Wand2,
  Info,
} from "lucide-react";
import { toast } from "sonner";
import { Sidebar } from "@/components/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";
import { SkillsBrandIcon } from "@/components/brand-kit-icons";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { listSkills, createSkill, updateSkill, deleteSkill } from "@/lib/api";
import type { Skill } from "@/lib/types";

/** Preset Skill Templates for fast injection */
const SKILL_TEMPLATES = [
  {
    name: "代码架构重构准则",
    description: "代码重构与解耦指导规范",
    content: `# 代码重构指导规范

## 核心职责
在进行代码重构时，遵循单一职责与强类型规范。

## 执行步骤
1. 审计当前代码文件，找出过于庞大的组件与重复逻辑。
2. 提取公共工具函数或子组件。
3. 确保不破坏原有导出的接口签名契约。`,
  },
  {
    name: "故障诊断准则",
    description: "堆栈日志与错误提取步骤",
    content: `# 故障诊断与调试规范

## 诊断要点
1. 始终优先读取完整错误日志，禁止凭空猜测。
2. 追踪数据提供方源头，禁止简单的捕获块静默吞掉错误。
3. 提供具体的根因分析与修正代码对照。`,
  },
  {
    name: "接口文档生成准则",
    description: "标准文档说明规范",
    content: `# 接口文档生成规范

## 输出要求
- 生成标准 Markdown 格式文档。
- 包含请求头、参数列表及响应示例。
- 针对边界错误输出明确的状态码说明。`,
  },
];

export default function SkillsPage() {
  const [skills, setSkills] = useState<Skill[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [activeSkill, setActiveSkill] = useState<Skill | null | undefined>(undefined);

  const refresh = async () => {
    try {
      setSkills(await listSkills());
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  useEffect(() => {
    refresh();
  }, []);

  const onDelete = async (s: Skill) => {
    setSkills((prev) => prev?.filter((x) => x.id !== s.id) ?? null);
    await deleteSkill(s.id);
    toast.success(`技能 "${s.name}" 已从库中注销`);
    await refresh();
  };

  const filteredSkills = useMemo(() => {
    if (!skills) return [];
    const q = query.trim().toLowerCase();
    if (!q) return skills;
    return skills.filter(
      (s) =>
        s.name.toLowerCase().includes(q) ||
        (s.description ?? "").toLowerCase().includes(q) ||
        s.content.toLowerCase().includes(q),
    );
  }, [skills, query]);

  const totalChars = useMemo(() => {
    return skills?.reduce((acc, s) => acc + s.content.length, 0) ?? 0;
  }, [skills]);

  return (
    <div className="flex h-screen bg-background text-foreground overflow-hidden selection:bg-teal-500/20">
      <Sidebar />
      <div className="flex flex-1 flex-col min-w-0 overflow-hidden">
        {/* Header - Pure Chinese */}
        <header className="flex h-12 shrink-0 items-center justify-between border-b border-border/80 px-4 bg-background/80 backdrop-blur">
          <div className="flex items-center gap-2.5">
            <div className="flex size-7 items-center justify-center rounded-md bg-teal-500/10 text-teal-600 dark:text-teal-400 ring-1 ring-teal-500/20">
              <SkillsBrandIcon size={16} />
            </div>
            <div className="flex items-center gap-2">
              <span className="text-sm font-semibold tracking-tight">技能知识库</span>
              <span className="text-xs text-muted-foreground font-medium">/ 技能</span>
            </div>
          </div>
          <ThemeToggle />
        </header>

        <main className="flex-1 overflow-y-auto p-6 space-y-6">
          <div className="mx-auto max-w-5xl space-y-6">
            {/* Banner - Pure Chinese */}
            <div className="relative overflow-hidden rounded-2xl border border-border/70 bg-card p-6 shadow-xs">
              <div className="absolute right-0 top-0 -mr-16 -mt-16 size-64 rounded-full bg-teal-500/5 blur-3xl pointer-events-none" />
              <div className="relative z-10 flex flex-col gap-5 md:flex-row md:items-center md:justify-between">
                <div className="space-y-1.5 max-w-xl">
                  <div className="flex items-center gap-2">
                    <span className="inline-flex items-center gap-1 rounded-md bg-teal-500/10 px-2 py-0.5 text-xs font-medium text-teal-600 dark:text-teal-400 border border-teal-500/20">
                      <Cpu className="size-3" /> 能力扩展矩阵
                    </span>
                    <span className="text-xs text-muted-foreground font-medium">能力扩展矩阵</span>
                  </div>
                  <h1 className="text-xl font-bold tracking-tight text-foreground">
                    智能体技能库与领域指导规范
                  </h1>
                  <p className="text-xs text-muted-foreground leading-relaxed">
                    定义专有领域技能。当智能体接收相关任务指令时，可动态检索并载入技能指导规范与标准操作流程。
                  </p>
                </div>

                <div className="flex flex-wrap items-center gap-3 shrink-0">
                  <div className="rounded-xl bg-muted/40 p-3 border border-border/60 text-right">
                    <div className="text-[10px] font-medium text-muted-foreground">技能总数</div>
                    <div className="text-base font-bold font-mono text-foreground">{skills?.length ?? 0} 个技能</div>
                  </div>
                  <Button
                    size="sm"
                    className="gap-1.5 bg-teal-600 text-white hover:bg-teal-700 dark:bg-teal-500 dark:hover:bg-teal-600 font-medium shadow-xs"
                    onClick={() => setActiveSkill(null)}
                  >
                    <Plus className="size-4" />
                    新建领域技能
                  </Button>
                </div>
              </div>
            </div>

            {/* Filter & Search - Pure Chinese */}
            <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
              <div className="flex items-center gap-2 text-xs text-muted-foreground font-medium">
                <span>已载入技能</span>
                <span className="rounded bg-muted px-2 py-0.5 text-foreground font-semibold font-mono">
                  {skills?.length ?? 0}
                </span>
              </div>

              <div className="relative max-w-xs flex-1">
                <Search className="absolute left-3 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
                <Input
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder="按技能名称或描述检索..."
                  className="h-8 pl-9 text-xs bg-card"
                />
              </div>
            </div>

            {error && (
              <div className="rounded-xl border border-destructive/40 bg-destructive/10 p-3.5 text-xs text-destructive">
                {error}
              </div>
            )}

            {/* Skeletons */}
            {skills === null && !error && (
              <div className="grid grid-cols-1 gap-3.5 sm:grid-cols-2">
                {Array.from({ length: 4 }).map((_, i) => (
                  <div key={i} className="h-32 animate-pulse rounded-xl border border-border/60 bg-muted/20" />
                ))}
              </div>
            )}

            {/* Empty State */}
            {skills !== null && skills.length === 0 && (
              <div className="flex flex-col items-center justify-center rounded-2xl border border-dashed border-border p-12 text-center">
                <div className="flex size-12 items-center justify-center rounded-2xl bg-teal-500/10 text-teal-500">
                  <SkillsBrandIcon size={24} />
                </div>
                <h3 className="mt-3 text-sm font-semibold">技能库暂未录入条目</h3>
                <p className="mt-1 text-xs text-muted-foreground max-w-xs leading-normal">
                  添加首个领域技能（如代码重构、架构审计、文档生成指南）。
                </p>
                <Button
                  size="sm"
                  className="gap-1.5 bg-teal-600 hover:bg-teal-700 text-white mt-4"
                  onClick={() => setActiveSkill(null)}
                >
                  <Plus className="size-4" />
                  新建首个技能
                </Button>
              </div>
            )}

            {skills !== null && skills.length > 0 && filteredSkills.length === 0 && (
              <div className="rounded-xl border border-dashed border-border py-12 text-center text-xs text-muted-foreground">
                未找到匹配“{query}”的技能条目。
              </div>
            )}

            {/* Skills Grid */}
            {skills !== null && filteredSkills.length > 0 && (
              <div className="grid grid-cols-1 gap-3.5 sm:grid-cols-2">
                {filteredSkills.map((skill) => (
                  <SkillCard
                    key={skill.id}
                    skill={skill}
                    onEdit={() => setActiveSkill(skill)}
                    onDelete={() => onDelete(skill)}
                  />
                ))}
              </div>
            )}
          </div>
        </main>
      </div>

      <SkillEditorDialog
        skill={activeSkill}
        open={activeSkill !== undefined}
        onClose={() => setActiveSkill(undefined)}
        onSaved={() => {
          setActiveSkill(undefined);
          refresh();
        }}
      />
    </div>
  );
}

function SkillCard({
  skill,
  onEdit,
  onDelete,
}: {
  skill: Skill;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const [copied, setCopied] = useState(false);

  const copyContent = () => {
    navigator.clipboard.writeText(skill.content);
    setCopied(true);
    toast.success(`已复制技能指令: ${skill.name}`);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="group relative flex flex-col justify-between rounded-xl border border-border/80 bg-card p-4 transition-all duration-200 hover:-translate-y-0.5 hover:border-teal-500/40 hover:shadow-md">
      <div>
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0">
            <h3 className="font-semibold text-sm tracking-tight text-foreground truncate group-hover:text-teal-600 dark:group-hover:text-teal-400 transition-colors">
              {skill.name}
            </h3>
            <p className="mt-0.5 text-xs text-muted-foreground line-clamp-1">
              {skill.description || "基础能力技能描述。"}
            </p>
          </div>
          <span className="shrink-0 rounded bg-muted px-1.5 py-0.2 font-mono text-[10px] text-muted-foreground border border-border/40">
            {skill.content.length} 字符
          </span>
        </div>

        <p className="mt-2.5 text-xs font-mono text-muted-foreground/90 line-clamp-2 leading-relaxed bg-muted/30 p-2 rounded-lg border border-border/40">
          {skill.content}
        </p>
      </div>

      <div className="mt-4 pt-3 border-t border-border/40 flex items-center justify-between">
        <button
          onClick={copyContent}
          className="flex items-center gap-1 text-[11px] text-muted-foreground hover:text-foreground transition-colors"
        >
          {copied ? <Check className="size-3 text-emerald-500" /> : <Copy className="size-3" />}
          <span>{copied ? "已复制" : "复制技能指令"}</span>
        </button>

        <div className="flex items-center gap-1">
          <Button
            size="sm"
            variant="ghost"
            className="h-7 px-2 text-xs gap-1 hover:bg-muted"
            onClick={onEdit}
          >
            <Pencil className="size-3" />
            编辑
          </Button>
          <Button
            size="sm"
            variant="ghost"
            className="h-7 px-2 text-xs gap-1 text-destructive hover:bg-destructive/10 hover:text-destructive"
            onClick={onDelete}
          >
            <Trash2 className="size-3" />
            移除
          </Button>
        </div>
      </div>
    </div>
  );
}

function SkillEditorDialog({
  skill,
  open,
  onClose,
  onSaved,
}: {
  skill: Skill | null | undefined;
  open: boolean;
  onClose: () => void;
  onSaved: () => void;
}) {
  const isEdit = Boolean(skill?.id);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [content, setContent] = useState("");
  const [mode, setMode] = useState<"edit" | "preview">("edit");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (skill) {
      setName(skill.name);
      setDescription(skill.description ?? "");
      setContent(skill.content);
    } else {
      setName("");
      setDescription("");
      setContent("");
    }
    setMode("edit");
    setError(null);
  }, [skill]);

  const lineCount = useMemo(() => (content ? content.split("\n").length : 0), [content]);
  const estimatedTokens = useMemo(() => Math.ceil(content.length / 3.5), [content]);

  const injectTemplate = (tmpl: typeof SKILL_TEMPLATES[number]) => {
    if (!name) setName(tmpl.name);
    if (!description) setDescription(tmpl.description);
    setContent((prev) => (prev ? `${prev}\n\n${tmpl.content}` : tmpl.content));
    toast.success(`已注入模板: ${tmpl.name}`);
  };

  const onSave = async () => {
    const n = name.trim();
    const c = content.trim();
    if (!n || !c) return;
    setSaving(true);
    setError(null);
    try {
      if (isEdit && skill) {
        await updateSkill(skill.id, {
          name: n,
          description: description.trim() || null,
          content: c,
        });
        toast.success(`技能 "${n}" 已更新`);
      } else {
        await createSkill({
          name: n,
          description: description.trim() || null,
          content: c,
        });
        toast.success(`技能 "${n}" 已成功录入`);
      }
      onSaved();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => { if (!o) onClose(); }}>
      <DialogContent className="sm:max-w-3xl rounded-2xl p-0 overflow-hidden border-border bg-card">
        {/* Dialog Header - Pure Chinese */}
        <div className="flex items-center justify-between border-b border-border/80 px-6 py-4 bg-muted/20">
          <div className="flex items-center gap-3">
            <div className="flex size-9 items-center justify-center rounded-xl bg-teal-500/10 text-teal-600 dark:text-teal-400 border border-teal-500/20">
              <SkillsBrandIcon size={18} />
            </div>
            <div>
              <DialogTitle className="text-base font-semibold tracking-tight text-foreground">
                {isEdit ? "配置领域技能" : "注册新智能体技能"}
              </DialogTitle>
              <DialogDescription className="text-xs text-muted-foreground mt-0.5">
                定义智能体可调用的领域专业指导与操作规范。
              </DialogDescription>
            </div>
          </div>

          <div className="flex items-center gap-1 rounded-xl bg-muted/60 p-1 border border-border/50">
            <button
              type="button"
              onClick={() => setMode("edit")}
              className={`flex items-center gap-1.5 rounded-lg px-3 py-1 text-xs font-medium transition-all ${
                mode === "edit"
                  ? "bg-background text-foreground shadow-2xs font-semibold"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              <Edit3 className="size-3.5 text-teal-500" />
              文档编辑
            </button>
            <button
              type="button"
              onClick={() => setMode("preview")}
              className={`flex items-center gap-1.5 rounded-lg px-3 py-1 text-xs font-medium transition-all ${
                mode === "preview"
                  ? "bg-background text-foreground shadow-2xs font-semibold"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              <Eye className="size-3.5 text-blue-500" />
              渲染效果
            </button>
          </div>
        </div>

        {/* Dialog Body - Pure Chinese */}
        <div className="p-6 space-y-4 max-h-[75vh] overflow-y-auto">
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <div className="space-y-1.5">
              <label className="text-[11px] font-semibold text-muted-foreground uppercase tracking-wider">
                技能名称
              </label>
              <Input
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="例如: 代码架构重构规范"
                className="text-xs bg-background"
                autoFocus
              />
            </div>
            <div className="space-y-1.5">
              <label className="text-[11px] font-semibold text-muted-foreground uppercase tracking-wider">
                功能摘要
              </label>
              <Input
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder="例如: 前端 UI 审计与重构技能指南"
                className="text-xs bg-background"
              />
            </div>
          </div>

          <div className="space-y-1.5">
            <div className="flex items-center justify-between text-[11px] text-muted-foreground">
              <span className="flex items-center gap-1">
                <Wand2 className="size-3 text-teal-500" /> 标准模板库
              </span>
              <span>点击快速导入模板</span>
            </div>
            <div className="flex flex-wrap gap-2">
              {SKILL_TEMPLATES.map((tmpl) => (
                <button
                  key={tmpl.name}
                  type="button"
                  onClick={() => injectTemplate(tmpl)}
                  className="flex items-center gap-1.5 rounded-lg border border-border/70 bg-muted/30 px-2.5 py-1 text-xs font-medium text-muted-foreground hover:border-teal-500/40 hover:text-foreground transition-all"
                >
                  <Zap className="size-3 text-teal-500" />
                  <span>{tmpl.name}</span>
                </button>
              ))}
            </div>
          </div>

          <div className="space-y-1.5">
            <div className="flex items-center justify-between text-[11px] text-muted-foreground">
              <span>技能指令说明</span>
              <div className="flex items-center gap-3">
                <span>{lineCount} 行</span>
                <span>{content.length} 字符</span>
                <span className="text-teal-600 dark:text-teal-400 font-semibold">
                  约 {estimatedTokens} 令牌
                </span>
              </div>
            </div>

            {mode === "edit" ? (
              <Textarea
                value={content}
                onChange={(e) => setContent(e.target.value)}
                placeholder="输入技能详细的操作流程与格式规范..."
                rows={10}
                className="font-mono text-xs leading-relaxed resize-none bg-background/80 focus-visible:ring-teal-500/30"
              />
            ) : (
              <div className="min-h-[220px] rounded-xl border border-border/70 bg-muted/20 p-4 font-mono text-xs leading-relaxed text-foreground whitespace-pre-wrap">
                {content || <span className="text-muted-foreground italic">（正文暂无内容，请切换到编辑模式输入）</span>}
              </div>
            )}
          </div>

          {error && (
            <div className="rounded-xl border border-destructive/40 bg-destructive/10 p-3 text-xs text-destructive flex items-center gap-2">
              <Info className="size-4 shrink-0" />
              {error}
            </div>
          )}
        </div>

        {/* Dialog Footer - Pure Chinese */}
        <div className="flex items-center justify-between border-t border-border/80 px-6 py-3.5 bg-muted/30">
          <span className="text-[11px] text-muted-foreground">
            {isEdit ? "更新后智能体调用该技能将自动生效新规范" : "录入保存后可分发至指定的智能体实例"}
          </span>

          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={onClose}
              className="text-xs"
            >
              取消
            </Button>
            <Button
              size="sm"
              onClick={onSave}
              disabled={saving || !name.trim() || !content.trim()}
              className="bg-teal-600 hover:bg-teal-700 text-white font-medium text-xs gap-1.5"
            >
              {saving ? (
                <>
                  <Loader2 className="size-3.5 animate-spin" />
                  保存写入中…
                </>
              ) : isEdit ? (
                "提交更新"
              ) : (
                "保存技能到知识库"
              )}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
