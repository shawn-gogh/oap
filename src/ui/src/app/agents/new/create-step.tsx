"use client";

import { useState } from "react";
import {
  ArrowUp,
  Bell,
  Bot,
  Database,
  FileText,
  LifeBuoy,
  Loader2,
  Mail,
  Search,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { AGENT_TEMPLATES } from "@/lib/agent-builder";
import type { AgentDraft, AgentTemplate } from "@/lib/agent-builder";
import type { AgentRuntime, RuntimeHarness, Skill } from "@/lib/types";
import { cn } from "@/lib/utils";
import { runtimeChoicesForDrafting } from "./builder-shared";
import { StreamingPreview } from "./streaming-preview";

const TEMPLATE_ICONS: Record<string, LucideIcon> = {
  blank: Bot,
  "deep-researcher": Search,
  "inbox-triage": Mail,
  "security-reviewer": ShieldCheck,
  "support-agent": LifeBuoy,
  "incident-commander": Bell,
  "data-analyst": Database,
  "sprint-retro": FileText,
};

export function CreateStep({
  draft,
  drafting,
  draftProgress,
  modelsError,
  modelsLoading,
  prompt,
  harnesses,
  selectedTemplateId,
  skills,
  runtimes,
  onDraftChange,
  onPromptChange,
  onGenerate,
  onStartFromUi,
  onTemplateSelect,
}: {
  draft: AgentDraft;
  drafting: boolean;
  draftProgress: string | null;
  modelsError: string | null;
  modelsLoading: boolean;
  prompt: string;
  harnesses: RuntimeHarness[];
  selectedTemplateId: string;
  skills: Skill[];
  runtimes: AgentRuntime[];
  onDraftChange: (next: AgentDraft) => void;
  onPromptChange: (next: string) => void;
  onGenerate: () => void;
  onStartFromUi: () => void;
  onTemplateSelect: (template: AgentTemplate) => void;
}) {
  void onDraftChange;
  const connectedRuntimes = runtimeChoicesForDrafting(harnesses, runtimes);

  return (
    <div className="grid min-h-[calc(100vh-6.5rem)] gap-6 px-4 py-6 lg:grid-cols-[minmax(420px,1fr)_minmax(520px,0.98fr)]">
      <section className="relative flex min-h-[560px] flex-col rounded-lg border border-transparent px-2 pb-2 sm:px-4">
        <div className="flex flex-1 items-center justify-center pb-24 text-center">
          {drafting ? (
            <div className="grid w-full max-w-2xl justify-items-center gap-5">
              <div className="ml-auto max-w-[82%] whitespace-pre-wrap break-words rounded-lg bg-foreground px-4 py-3 text-left text-sm text-background">
                {prompt.trim()}
              </div>
              <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
                <Loader2 className="size-4 animate-spin text-foreground motion-reduce:animate-none" />
                {draftProgress ? "正在生成 config.yaml" : "正在分析需求..."}
              </div>
              {draftProgress && <StreamingPreview text={draftProgress} />}
            </div>
          ) : (
            <div>
              <h1 className="text-2xl font-semibold text-foreground">描述要部署的智能体应用</h1>
              <p className="mt-4 text-base text-muted-foreground">
                说明业务目标、输入、交付结果和禁止动作，系统会先生成应用蓝图，再配置执行能力。
              </p>
              <div className="mt-6 grid gap-2 text-left sm:grid-cols-3">
                <ConversationHint
                  title="运行时"
                  value={`${connectedRuntimes.length || 0} 个可选`}
                  detail="优先选择已连接的运行时"
                />
                <ConversationHint title="模型" value="自动推荐" detail="按任务复杂度和可用模型选择" />
                <ConversationHint title="技能" value={`${skills.length} 个可用`} detail="只附加真正相关的技能" />
              </div>
            </div>
          )}
        </div>

        <div className="mx-auto w-full max-w-3xl overflow-hidden rounded-lg border border-border bg-card shadow-[0_18px_70px_rgba(15,23,42,0.10)]">
          <Textarea
            value={prompt}
            onChange={(event) => onPromptChange(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey && !drafting) {
                event.preventDefault();
                onGenerate();
              }
            }}
            placeholder="例如：每个工作日上午整理客服邮箱，在 Slack 交付优先级报告和回复草稿，但不要直接发送邮件。"
            className="min-h-24 resize-none border-0 bg-transparent px-4 py-4 text-[15px] text-foreground shadow-none outline-none placeholder:text-muted-foreground focus-visible:ring-0"
          />
          <div className="flex flex-wrap items-center gap-2 border-t border-border bg-muted/30 px-3 py-3">
            <span className="text-xs text-muted-foreground">
              {modelsLoading ? "正在读取可用模型..." : "先生成应用蓝图，再推荐运行时、模型、工具和技能"}
            </span>
            {modelsError && <span className="text-xs text-destructive">{modelsError}</span>}
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={onStartFromUi}
              disabled={drafting}
              className="gap-1.5"
            >
              <Bot className="size-3.5" />
              使用表单编辑器
            </Button>
            <div className="ml-auto" />
            <Button
              type="button"
              size="icon-sm"
              onClick={onGenerate}
              disabled={!prompt.trim() || drafting}
              className="size-9 rounded-full"
              aria-label="生成配置"
            >
              {drafting ? (
                <Loader2 className="size-4 animate-spin motion-reduce:animate-none" />
              ) : (
                <ArrowUp className="size-4" />
              )}
            </Button>
          </div>
        </div>
      </section>

      <section className="min-h-0">
        <TemplateBrowser selectedTemplateId={selectedTemplateId} onSelect={onTemplateSelect} />
      </section>
    </div>
  );
}

function ConversationHint({ title, value, detail }: { title: string; value: string; detail: string }) {
  return (
    <div className="rounded-lg border border-border bg-card/70 px-3 py-2">
      <div className="text-xs text-muted-foreground">{title}</div>
      <div className="mt-1 text-sm font-semibold">{value}</div>
      <div className="mt-1 text-xs leading-4 text-muted-foreground">{detail}</div>
    </div>
  );
}

function TemplateBrowser({
  selectedTemplateId,
  onSelect,
}: {
  selectedTemplateId: string;
  onSelect: (template: AgentTemplate) => void;
}) {
  const [query, setQuery] = useState("");
  const normalized = query.trim().toLowerCase();
  const templates = normalized
    ? AGENT_TEMPLATES.filter((template) =>
        [template.title, template.description, ...template.tags, template.draft.name]
          .join(" ")
          .toLowerCase()
          .includes(normalized),
      )
    : AGENT_TEMPLATES;

  return (
    <div className="flex h-full min-h-[560px] flex-col rounded-lg border border-border bg-card p-5 shadow-sm">
      <div className="mb-4">
        <h2 className="text-lg font-semibold text-foreground">浏览模板</h2>
        <div className="relative mt-4">
          <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="搜索模板"
            className="h-10 pl-9"
          />
        </div>
      </div>
      <div className="grid min-h-0 flex-1 gap-3 overflow-y-auto pr-1 sm:grid-cols-2">
        {templates.map((template) => {
          const Icon = TEMPLATE_ICONS[template.id] ?? Sparkles;
          const selected = template.id === selectedTemplateId;
          return (
            <button
              key={template.id}
              type="button"
              onClick={() => onSelect(template)}
              className={cn(
                "min-h-28 rounded-lg border border-border bg-background p-4 text-left transition hover:bg-muted/40",
                selected && "border-foreground ring-2 ring-foreground/10",
              )}
            >
              <div className="flex min-h-full flex-col">
                <div className="flex items-start gap-3">
                  <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-muted text-foreground">
                    <Icon className="size-4" />
                  </span>
                  <span className="min-w-0">
                    <span className="block text-sm font-semibold text-foreground">{template.title}</span>
                    <span className="mt-1 line-clamp-2 block text-xs leading-5 text-muted-foreground">
                      {template.description}
                    </span>
                  </span>
                </div>
                <div className="mt-auto flex flex-wrap gap-1.5 pt-4">
                  {template.tags.map((tag) => (
                    <span
                      key={tag}
                      className="rounded-md border border-border bg-muted/40 px-2 py-0.5 text-[11px] text-muted-foreground"
                    >
                      {tag}
                    </span>
                  ))}
                </div>
              </div>
            </button>
          );
        })}
      </div>
    </div>
  );
}
