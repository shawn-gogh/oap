"use client";

import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import {
  Activity,
  GitPullRequest,
  MoreHorizontal,
  RefreshCw,
  RotateCcw,
  ShieldCheck,
} from "lucide-react";
import { useConfirm } from "@/components/confirm-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  activateAgent,
  apiErrorMessage,
  checkAgentConformance,
  checkAgentHealth,
  emergencyStopAgent,
  getAgent,
  getAgentByoCredentialStatus,
  getAgentGovernance,
  getAgentSource,
  preflightAgent,
  probeAgentSourceRuntimeMapping,
  requestAgentPublish,
  resolveAgentDrift,
  retireAgent,
  rollbackAgent,
  saveAgentByoCredential,
  setAgentSourceRuntimeMapping,
  suggestAgentSourceRuntimeMapping,
  syncAgentSource,
  testAgentGovernance,
  type AgentGovernanceResponse,
  type AgentPreflightReport,
  type AgentSourceOverview,
  type RuntimeMapping,
  type RuntimeMappingProbe,
  type RuntimePathSuggestion,
} from "@/lib/api";
import { flattenJsonPointers } from "@/lib/json-pointer-tree";
import {
  isFieldUndeclared,
  mappingFieldCandidates,
  unobservedFields,
  MAPPING_ORIGIN_LABELS,
  type MappingFieldCandidate,
  type MappingFieldOrigin,
} from "@/lib/mapping-fields";
import { useCurrentTime } from "@/lib/use-current-time";
import type { Agent } from "@/lib/types";

const PREFLIGHT_VERDICT_META: Record<string, { label: string; className: string }> = {
  verified: { label: "已验证", className: "bg-emerald-500/10 text-emerald-700 dark:text-emerald-400" },
  exists_only: { label: "仅存在性", className: "bg-muted text-muted-foreground" },
  unverified: { label: "未验证", className: "bg-amber-500/10 text-amber-700 dark:text-amber-400" },
  failed: { label: "失败", className: "bg-destructive/10 text-destructive" },
};

const MANAGEMENT_MODE_LABELS: Record<string, string> = {
  federated: "联邦接入",
  mirrored: "镜像托管",
  managed: "平台托管",
};

// These three bridges execute against a provider-specific I/O shape they
// can't infer on their own — sessions::external_bridge won't run a session
// until an operator has confirmed the mapping (config.source.raw["x-lap-runtime"]).
const RUNTIME_MAPPING_PROVIDERS = new Set(["openapi", "langgraph", "crewai"]);

// Providers whose mapping can be confirmed against an observed payload. CrewAI
// is absent because its bridge is an async kickoff plus a session-bound polling
// loop, which a probe would have to reimplement rather than reuse.
const PROBE_PROVIDERS = new Set(["langgraph", "openapi"]);

/** Mapping fields whose provenance is worth surfacing, with their labels. */
const ORIGIN_TRACKED_FIELDS = ["input_field", "output_field", "output_path"] as const;
type OriginTrackedField = (typeof ORIGIN_TRACKED_FIELDS)[number];
const FIELD_LABELS: Record<OriginTrackedField, string> = {
  input_field: "请求字段",
  output_field: "响应字段",
  output_path: "响应字段路径",
};

/** Where a filled-in value came from — claimed by the spec, or observed. */
function OriginBadge({ origin }: { origin?: MappingFieldOrigin }) {
  if (!origin) return null;
  return (
    <span
      className={`rounded px-1 text-[10px] font-normal ${
        origin === "probe"
          ? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-400"
          : "bg-muted text-muted-foreground"
      }`}
    >
      {MAPPING_ORIGIN_LABELS[origin]}
    </span>
  );
}

/** Field names the source's own schema declares, offered instead of free text. */
function SchemaFieldPicker({
  candidates,
  selected,
  onSelect,
}: {
  candidates: MappingFieldCandidate[];
  selected: string;
  onSelect: (name: string) => void;
}) {
  if (candidates.length === 0) return null;
  return (
    <div className="max-h-28 overflow-auto rounded border border-border bg-muted/40 p-1">
      {candidates.map((candidate) => (
        <button
          key={candidate.name}
          type="button"
          onClick={() => onSelect(candidate.name)}
          className={`flex w-full items-baseline gap-2 rounded px-1 py-0.5 text-left hover:bg-muted ${
            selected === candidate.name ? "bg-primary/10" : ""
          }`}
        >
          <span className="shrink-0 font-mono text-[11px]">{candidate.name}</span>
          <span className="shrink-0 text-[11px] text-muted-foreground">{candidate.typeLabel}</span>
          {candidate.required && (
            <span className="shrink-0 text-[11px] text-muted-foreground">必填</span>
          )}
          {candidate.title && (
            <span className="truncate text-[11px] text-muted-foreground">{candidate.title}</span>
          )}
        </button>
      ))}
    </div>
  );
}

const SYNC_STATE_LABELS: Record<string, string> = {
  unknown: "未知",
  in_sync: "已同步",
  drifted: "有漂移",
  missing: "来源缺失",
  sync_error: "同步失败",
  detached: "已断开",
};

const CONFORMANCE_STATUS_LABELS: Record<string, string> = {
  unknown: "未知",
  conformant: "符合契约",
  partial: "部分符合",
  non_conformant: "不符合契约",
};

const DRIFT_RISK_LABELS: Record<string, string> = {
  low: "低",
  medium: "中",
  high: "高",
  critical: "严重",
};

/** Compact single-line preview of a drift value for the old→new diff row. */
function driftValuePreview(value: unknown): string {
  if (value === undefined || value === null) return "（空）";
  const text = typeof value === "string" ? value : JSON.stringify(value);
  return text.length > 60 ? `${text.slice(0, 57)}…` : text;
}

/** Full (but bounded) rendering of a drift value for the review dialog. */
function driftValueFull(value: unknown): string {
  if (value === undefined || value === null) return "（空）";
  const text = typeof value === "string" ? value : JSON.stringify(value, null, 2);
  return text.length > 1200 ? `${text.slice(0, 1200)}…` : text;
}

const HEALTH_FRESHNESS_MS = 24 * 60 * 60 * 1000;

/** Compact Chinese relative-time label, e.g. "3 分钟前" / "2 小时前" / "5 天前". */
function relativeTimeLabel(ms: number): string {
  const diff = Math.max(0, Date.now() - ms);
  const minutes = Math.floor(diff / 60000);
  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  return `${Math.floor(hours / 24)} 天前`;
}

interface HealthSignal {
  healthy: boolean;
  checkedAt: number;
  /** manual = someone clicked 运行检查; auto = the scheduler's periodic check. */
  origin: "manual" | "auto";
}

/** The most recent known health outcome, manual or automatic, whichever is
 *  newer. `governance.last_health_at` only reflects manual "运行检查" clicks
 *  (mark_tested); the scheduler's automatic checks land in
 *  `source.recent_health_checks` (check_kind "preflight" is the per-run
 *  summary) without ever touching the governance row. Reading only the
 *  governance field made the status card blind to automatic monitoring —
 *  it could say "运行正常" hours after an automatic check had already found
 *  a problem. */
function latestHealthSignal(
  governance: AgentGovernanceResponse["governance"],
  source: AgentSourceOverview | null,
): HealthSignal | null {
  const manual: HealthSignal | null =
    governance.last_health_at != null
      ? {
          healthy: governance.runtime_health === "healthy",
          checkedAt: governance.last_health_at,
          origin: "manual",
        }
      : null;
  const autoChecks = (source?.recent_health_checks ?? []).filter(
    (check) => check.check_kind === "preflight",
  );
  const auto: HealthSignal | null =
    autoChecks.length > 0
      ? (() => {
          const latest = autoChecks.reduce((a, b) => (a.checked_at > b.checked_at ? a : b));
          return { healthy: latest.status === "healthy", checkedAt: latest.checked_at, origin: "auto" as const };
        })()
      : null;
  if (!manual) return auto;
  if (!auto) return manual;
  return auto.checkedAt >= manual.checkedAt ? auto : manual;
}

type GovernancePrimaryAction = "test" | "publish" | "inbox" | "activate" | "drift";

interface GovernanceUx {
  tone: "ok" | "warn" | "error" | "muted";
  status: string;
  reason: string;
  primary: { action: GovernancePrimaryAction; label: string } | null;
}

/** Collapses the four raw state machines (agent status × lifecycle × health ×
 *  sync) into one plain-language status plus the single next action the user
 *  should take. The pipeline stays visible in the stage bar; this answers
 *  "现在能不能用？下一步做什么？" without making users decode enums. */
function deriveGovernanceUx(input: {
  agentStatus: string;
  governance: AgentGovernanceResponse["governance"];
  currentRevision: number;
  evalGate: AgentGovernanceResponse["eval_gate"];
  hasDriftCandidate: boolean;
  health: HealthSignal | null;
  healthFresh: boolean;
}): GovernanceUx {
  const {
    agentStatus,
    governance,
    currentRevision,
    evalGate,
    hasDriftCandidate,
    health,
    healthFresh,
  } = input;
  const detail = governance.health_detail?.trim() || null;
  switch (governance.lifecycle_status) {
    case "retired":
      return {
        tone: "muted",
        status: "已退役",
        reason: detail ?? "来源证据已保留，不能再进行会话或运行。",
        primary: null,
      };
    case "suspended":
      return {
        tone: "error",
        status: "已挂起",
        reason: detail ?? "紧急停止或健康检查失败。修复问题后重新运行检查即可解除。",
        primary: { action: "test", label: "重新运行检查" },
      };
    default:
      break;
  }
  if (hasDriftCandidate) {
    return {
      tone: "warn",
      status: "待评审来源变更",
      reason: "远端定义发生了变化。接受后将进入重新发布流程，拒绝则继续运行当前版本。",
      primary: { action: "drift", label: "评审来源变更" },
    };
  }
  switch (governance.lifecycle_status) {
    case "review_due":
      return {
        tone: "warn",
        status: "发布已到期，待复审",
        reason: "新工作已暂停。重新运行治理检查，通过后申请发布复审。",
        primary: { action: "test", label: "开始复审检查" },
      };
    case "pending_approval":
      return {
        tone: "warn",
        status: "等待发布审批",
        reason: "审批在收件箱完成；重新运行检查可撤回本次申请。",
        primary: { action: "inbox", label: "前往收件箱审批" },
      };
    case "published":
    case "rolled_back":
      if (agentStatus !== "active") {
        return {
          tone: "warn",
          status: governance.lifecycle_status === "rolled_back" ? "已回滚，未激活" : "已发布，未激活",
          reason:
            governance.lifecycle_status === "rolled_back"
              ? "配置已回滚到先前发布的版本。激活时会重新运行预检确认健康。"
              : "发布审批已通过。激活后即可开始会话和运行。",
          primary: { action: "activate", label: "激活" },
        };
      }
      if (health && !health.healthy && healthFresh) {
        return {
          tone: "warn",
          status: "运行中，最近检查异常",
          reason: `最近一次${health.origin === "manual" ? "手动" : "自动"}健康检查（${relativeTimeLabel(
            health.checkedAt,
          )}）未通过，建议重新运行检查确认当前状态。`,
          primary: { action: "test", label: "重新运行检查" },
        };
      }
      return {
        tone: "ok",
        status: "运行中",
        reason:
          health && healthFresh
            ? `已发布并激活，最近一次健康检查（${health.origin === "manual" ? "手动" : "自动"}，${relativeTimeLabel(
                health.checkedAt,
              )}）正常。`
            : "已发布并激活。平台会定期自动同步来源并运行健康检查。",
        primary: null,
      };
    case "unhealthy":
      return {
        tone: "error",
        status: "检查未通过",
        reason: detail ?? "最近一次运行检查存在阻断项，修复后重新检查。",
        primary: { action: "test", label: "重新运行检查" },
      };
    case "tested":
      if (governance.tested_revision === currentRevision) {
        if (!evalGate.passed) {
          return {
            tone: "warn",
            status: "待黄金评估",
            reason: evalGate.message,
            primary: null,
          };
        }
        return {
          tone: "ok",
          status: "检查通过",
          reason: evalGate.required
            ? "当前版本已通过运行检查和黄金用例回归，可以申请发布（需管理员审批）。"
            : "当前版本已通过运行检查；尚未定义黄金用例，申请发布时会给出提示。",
          primary: { action: "publish", label: "申请发布" },
        };
      }
      return {
        tone: "warn",
        status: "配置已变更",
        reason: "配置在上次检查之后有改动，需要对当前版本重新运行检查。",
        primary: { action: "test", label: "重新运行检查" },
      };
    default:
      return {
        tone: "warn",
        status: "待检查",
        reason: "运行检查会验证来源连通性、凭据与真实执行链路，是发布前的第一步。",
        primary: { action: "test", label: "运行检查" },
      };
  }
}

const GOVERNANCE_TONE_CLASS: Record<GovernanceUx["tone"], string> = {
  ok: "bg-emerald-500/10 text-emerald-700 dark:text-emerald-400",
  warn: "bg-amber-500/10 text-amber-700 dark:text-amber-400",
  error: "bg-destructive/10 text-destructive",
  muted: "bg-muted text-muted-foreground",
};

export function ManagedGovernancePanel({
  response,
  agentStatus,
  grantsCount,
  onChange,
  onAgentChange,
  onReport,
}: {
  response: AgentGovernanceResponse;
  agentStatus: string;
  grantsCount: number;
  onChange: (response: AgentGovernanceResponse) => void;
  onAgentChange: (agent: Agent) => void;
  onReport: (report: AgentPreflightReport) => void;
}) {
  const currentTime = useCurrentTime();
  const confirmAction = useConfirm();
  const [busy, setBusy] = useState<"test" | "publish" | "rollback" | "sync" | "conformance" | "health" | "accept" | "reject" | "stop" | "retire" | "activate" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [source, setSource] = useState<AgentSourceOverview | null>(null);
  const [byoConfigured, setByoConfigured] = useState<boolean | null>(null);
  const [lastReport, setLastReport] = useState<{ kind: "运行检查" | "健康检查"; report: AgentPreflightReport } | null>(null);
  const [driftDialogOpen, setDriftDialogOpen] = useState(false);
  const [driftReason, setDriftReason] = useState("");
  const [byoDialogOpen, setByoDialogOpen] = useState(false);
  const [byoKeyDraft, setByoKeyDraft] = useState("");
  const [mappingDialogOpen, setMappingDialogOpen] = useState(false);
  const [mappingDraft, setMappingDraft] = useState<RuntimeMapping>({});
  const [mappingSaving, setMappingSaving] = useState(false);
  const [mappingSuggestLoading, setMappingSuggestLoading] = useState(false);
  const [mappingSuggestNote, setMappingSuggestNote] = useState<string | null>(null);
  const [probeRunning, setProbeRunning] = useState(false);
  const [probe, setProbe] = useState<RuntimeMappingProbe | null>(null);
  const [probeError, setProbeError] = useState<string | null>(null);
  const [mappingPaths, setMappingPaths] = useState<RuntimePathSuggestion[]>([]);
  const [mappingOrigins, setMappingOrigins] = useState<
    Partial<Record<OriginTrackedField, MappingFieldOrigin>>
  >({});
  // Schemas were already captured with the mapping; these turn them from
  // write-only payload into the pick lists that replace free-text entry.
  const inputCandidates = useMemo(
    () => mappingFieldCandidates(mappingDraft.input_schema),
    [mappingDraft.input_schema],
  );
  const outputCandidates = useMemo(
    () => mappingFieldCandidates(mappingDraft.output_schema),
    [mappingDraft.output_schema],
  );
  const unconfirmedFields = useMemo(
    () => unobservedFields(mappingOrigins, ORIGIN_TRACKED_FIELDS),
    [mappingOrigins],
  );
  const governance = response.governance;
  const testedCurrentRevision =
    governance.runtime_health === "healthy" &&
    governance.tested_revision === response.current_revision;

  useEffect(() => {
    let cancelled = false;
    void getAgentSource(governance.agent_id)
      .then((next) => {
        if (!cancelled) setSource(next);
      })
      .catch(() => {
        if (!cancelled) setSource(null);
      });
    if (governance.credential_scope === "byo") {
      void getAgentByoCredentialStatus(governance.agent_id)
        .then((configured) => {
          if (!cancelled) setByoConfigured(configured);
        })
        .catch(() => {
          if (!cancelled) setByoConfigured(null);
        });
    }
    return () => {
      cancelled = true;
    };
  }, [governance.agent_id, governance.credential_scope]);

  const saveByoKey = async () => {
    const value = byoKeyDraft.trim();
    if (!value) return;
    try {
      await saveAgentByoCredential(governance.agent_id, value);
      setByoConfigured(true);
      setByoDialogOpen(false);
      setByoKeyDraft("");
      toast.success("已保存你的 BYO 密钥");
    } catch (e) {
      toast.error(apiErrorMessage(e, "保存密钥失败"));
    }
  };

  const openMappingDialog = async () => {
    setMappingDraft({});
    setMappingSuggestNote(null);
    setProbe(null);
    setProbeError(null);
    setMappingPaths([]);
    setMappingOrigins({});
    setMappingDialogOpen(true);
    setMappingSuggestLoading(true);
    try {
      const suggestion = await suggestAgentSourceRuntimeMapping(governance.agent_id);
      const paths = suggestion.paths ?? [];
      setMappingPaths(paths);
      // A spec declaring exactly one callable route leaves nothing to choose,
      // so apply it rather than making the operator click the only option.
      const sole = paths.length === 1 ? paths[0] : null;
      const input_field = sole?.input_field || suggestion.input_field || undefined;
      const output_field = sole?.output_field || undefined;
      const output_path = suggestion.output_path || undefined;
      setMappingDraft((draft) => ({
        ...draft,
        path: draft.path || sole?.path || undefined,
        input_field: draft.input_field || input_field,
        output_field: draft.output_field || output_field,
        output_path: draft.output_path || output_path,
        input_schema: sole?.input_schema ?? suggestion.input_schema ?? draft.input_schema,
        output_schema: sole?.output_schema ?? suggestion.output_schema ?? draft.output_schema,
      }));
      setMappingOrigins({
        ...(input_field ? { input_field: "spec" as const } : {}),
        ...(output_field ? { output_field: "spec" as const } : {}),
        ...(output_path ? { output_path: "spec" as const } : {}),
      });
      setMappingSuggestNote(suggestion.note);
    } catch (e) {
      setMappingSuggestNote(apiErrorMessage(e, "自动获取失败，请手动确认。"));
    } finally {
      setMappingSuggestLoading(false);
    }
  };

  const applyPathSuggestion = (candidate: RuntimePathSuggestion) => {
    setMappingDraft((draft) => ({
      ...draft,
      path: candidate.path,
      input_field: candidate.input_field ?? draft.input_field,
      output_field: candidate.output_field ?? draft.output_field,
      input_schema: candidate.input_schema ?? draft.input_schema,
      output_schema: candidate.output_schema ?? draft.output_schema,
    }));
    setMappingOrigins((origins) => ({
      ...origins,
      ...(candidate.input_field ? { input_field: "spec" as const } : {}),
      ...(candidate.output_field ? { output_field: "spec" as const } : {}),
    }));
    // A probe result belongs to the route it was run against.
    setProbe(null);
    setProbeError(null);
  };

  const setMappingField = (
    field: OriginTrackedField,
    value: string,
    origin: MappingFieldOrigin,
  ) => {
    setMappingDraft((draft) => ({ ...draft, [field]: value }));
    setMappingOrigins((origins) => ({ ...origins, [field]: origin }));
  };

  // Runs the source once for real so output_path is picked from an observed
  // payload. The platform still cannot judge which field is *safe* to expose
  // (an `output` holding internal reasoning looks identical to an `answer`
  // holding the reply) — this only removes the guesswork about the shape.
  const runProbe = async () => {
    setProbeRunning(true);
    setProbeError(null);
    try {
      const result = await probeAgentSourceRuntimeMapping(governance.agent_id, {
        inputField: mappingDraft.input_field?.trim() || undefined,
        path: mappingDraft.path?.trim() || undefined,
      });
      setProbe(result);
    } catch (e) {
      setProbe(null);
      setProbeError(apiErrorMessage(e, "试跑失败"));
    } finally {
      setProbeRunning(false);
    }
  };

  const saveMapping = async () => {
    setMappingSaving(true);
    try {
      await setAgentSourceRuntimeMapping(governance.agent_id, mappingDraft);
      onAgentChange(await getAgent(governance.agent_id));
      setMappingDialogOpen(false);
      toast.success("已确认执行映射，现在可以运行会话了");
    } catch (e) {
      toast.error(apiErrorMessage(e, "保存映射失败"));
    } finally {
      setMappingSaving(false);
    }
  };

  const runTest = async () => {
    setBusy("test");
    setError(null);
    try {
      const next = await testAgentGovernance(governance.agent_id);
      onChange(next);
      if (next.preflight) {
        onReport(next.preflight);
        setLastReport({ kind: "运行检查", report: next.preflight });
      }
      if (next.governance.runtime_health === "healthy") {
        // Collapse the ceremonial step: the check passed, so offer the next
        // pipeline action right in the confirmation instead of making the
        // user find the button.
        toast.success("运行检查通过", {
          action: { label: "申请发布", onClick: () => void requestPublish() },
        });
      } else {
        toast.error("运行检查未通过，详情见下方检查报告");
      }
    } catch (e) {
      setError(apiErrorMessage(e, "运行检查失败"));
    } finally {
      setBusy(null);
    }
  };

  const requestPublish = async () => {
    setBusy("publish");
    setError(null);
    try {
      const next = await requestAgentPublish(governance.agent_id);
      onChange({
        ...response,
        governance: next.governance,
        eval_gate: next.eval_gate,
      });
      toast.success("发布申请已提交，等待管理员审批");
      if (next.warnings[0]) toast.warning(next.warnings[0]);
    } catch (e) {
      setError(apiErrorMessage(e, "提交发布申请失败"));
    } finally {
      setBusy(null);
    }
  };

  const activate = async () => {
    setBusy("activate");
    setError(null);
    try {
      await activateAgent(governance.agent_id);
      onAgentChange(await getAgent(governance.agent_id));
      toast.success("智能体已激活");
    } catch (e) {
      setError(apiErrorMessage(e, "激活失败"));
    } finally {
      setBusy(null);
    }
  };

  const runPrimary = (action: GovernancePrimaryAction) => {
    if (action === "test") void runTest();
    if (action === "publish") void requestPublish();
    if (action === "activate") void activate();
    if (action === "inbox") window.location.assign("/inbox/");
    if (action === "drift") setDriftDialogOpen(true);
  };

  const rollback = async () => {
    const confirmed = await confirmAction({
      title: "回滚智能体版本",
      description: "回滚会恢复上一个已发布版本，并生成新的可审计版本记录。",
      confirmLabel: "确认回滚",
    });
    if (!confirmed) return;
    setBusy("rollback");
    setError(null);
    try {
      const next = await rollbackAgent(governance.agent_id);
      onAgentChange(next.agent);
      onChange(await getAgentGovernance(governance.agent_id));
      toast.success("智能体已回滚到上一个已发布版本");
    } catch (e) {
      setError(apiErrorMessage(e, "回滚失败"));
    } finally {
      setBusy(null);
    }
  };

  const refreshSource = async () => {
    const next = await getAgentSource(governance.agent_id);
    setSource(next);
    return next;
  };

  const runSourceAction = async (
    action: "sync" | "conformance" | "health" | "accept" | "reject",
  ) => {
    const reason =
      action === "accept" || action === "reject" ? driftReason.trim() || undefined : undefined;
    setBusy(action);
    setError(null);
    try {
      if (action === "sync") setSource(await syncAgentSource(governance.agent_id));
      if (action === "accept") setSource(await resolveAgentDrift(governance.agent_id, "accept", reason));
      if (action === "reject") setSource(await resolveAgentDrift(governance.agent_id, "reject", reason));
      if (action === "accept" || action === "reject") {
        setDriftDialogOpen(false);
        setDriftReason("");
        onChange(await getAgentGovernance(governance.agent_id));
      }
      if (action === "conformance") {
        await checkAgentConformance(governance.agent_id);
        await refreshSource();
      }
      if (action === "health") {
        const result = await checkAgentHealth(governance.agent_id);
        onReport(result.preflight);
        setLastReport({ kind: "健康检查", report: result.preflight });
        await refreshSource();
      }
      toast.success({
        sync: "来源同步完成",
        conformance: "运行时契约检查完成",
        health: "健康检查完成",
        accept: "来源变更已接受，智能体已回到草稿状态",
        reject: "来源变更已拒绝",
      }[action]);
    } catch (e) {
      setError(apiErrorMessage(e, "纳管操作失败"));
    } finally {
      setBusy(null);
    }
  };

  const stopOrRetire = async (action: "stop" | "retire") => {
    const confirmed = await confirmAction({
      title: action === "stop" ? "紧急停止智能体" : "退役智能体",
      description:
        action === "stop"
          ? "将暂停新工作、取消进行中的会话和运行，并撤销会话能力令牌。"
          : "将停止全部工作、撤销能力令牌、断开来源连接并保留审计证据。",
      confirmLabel: action === "stop" ? "紧急停止" : "确认退役",
      destructive: true,
    });
    if (!confirmed) return;
    setBusy(action);
    setError(null);
    try {
      if (action === "stop") await emergencyStopAgent(governance.agent_id);
      else await retireAgent(governance.agent_id);
      onAgentChange(await getAgent(governance.agent_id));
      onChange(await getAgentGovernance(governance.agent_id));
      await refreshSource();
      toast.success(action === "stop" ? "智能体已紧急停止" : "智能体已退役");
    } catch (e) {
      setError(apiErrorMessage(e, action === "stop" ? "紧急停止失败" : "退役失败"));
    } finally {
      setBusy(null);
    }
  };

  const published = ["published", "rolled_back"].includes(governance.lifecycle_status);
  const health = latestHealthSignal(governance, source);
  const healthFresh =
    health != null &&
    currentTime != null &&
    currentTime - health.checkedAt < HEALTH_FRESHNESS_MS;
  const ux = deriveGovernanceUx({
    agentStatus,
    governance,
    currentRevision: response.current_revision,
    evalGate: response.eval_gate,
    hasDriftCandidate: source?.candidate_snapshot != null,
    health,
    healthFresh,
  });
  const stages = [
    { label: "导入", done: true },
    { label: "测试", done: testedCurrentRevision },
    { label: "回归", done: response.eval_gate.passed },
    { label: "发布", done: published },
    // Authorized means someone actually holds a grant — publishing alone
    // grants nobody anything.
    { label: grantsCount > 0 ? `授权（${grantsCount}）` : "授权", done: published && grantsCount > 0 },
    // Monitored means a recent health check, not "checked once, ever".
    { label: "监控", done: healthFresh },
  ];

  return (
    <section>
      <div className="mb-2">
        <h2 className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
          <ShieldCheck className="size-3.5" />
          外部智能体纳管
        </h2>
        <p className="mt-1 text-xs text-muted-foreground">导入、测试、审批发布、授权和运行监控使用同一版本链路。</p>
      </div>
      <Card className="overflow-hidden">
        <div className="grid grid-cols-6 border-b border-border bg-muted/20">
          {stages.map((stage, index) => (
            <div key={stage.label} className="relative px-2 py-3 text-center">
              <div className={`mx-auto mb-1 flex size-6 items-center justify-center rounded-full text-xs font-semibold ${stage.done ? "bg-emerald-500 text-white" : "bg-muted text-muted-foreground"}`}>
                {index + 1}
              </div>
              <span className="text-[11px] text-muted-foreground">{stage.label}</span>
            </div>
          ))}
        </div>
        <div className="grid gap-4 p-4">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="min-w-0">
              <p className="flex flex-wrap items-center gap-2">
                <span className={`rounded px-2 py-0.5 text-xs font-semibold ${GOVERNANCE_TONE_CLASS[ux.tone]}`}>
                  {ux.status}
                </span>
              </p>
              <p className="mt-1.5 max-w-2xl text-xs text-muted-foreground">{ux.reason}</p>
            </div>
            <div className="flex items-center gap-2">
              {ux.primary && (
                <Button size="sm" disabled={busy !== null} onClick={() => runPrimary(ux.primary!.action)}>
                  {busy !== null && ["test", "publish", "activate"].includes(busy)
                    ? "处理中..."
                    : ux.primary.label}
                </Button>
              )}
              <DropdownMenu>
                <DropdownMenuTrigger
                  disabled={busy !== null}
                  aria-label="更多纳管操作"
                  className="inline-flex h-8 items-center gap-1.5 rounded-md border border-border bg-background px-3 text-xs font-medium shadow-xs transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50"
                >
                  <MoreHorizontal className="size-3.5" />更多
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="w-48">
                  <DropdownMenuItem onClick={() => void runTest()}>
                    <Activity className="size-3.5" />运行检查
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    disabled={
                      !testedCurrentRevision ||
                      !response.eval_gate.passed ||
                      governance.lifecycle_status === "pending_approval"
                    }
                    onClick={() => void requestPublish()}
                  >
                    <GitPullRequest className="size-3.5" />申请发布
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    disabled={governance.previous_published_revision == null}
                    onClick={() => void rollback()}
                  >
                    <RotateCcw className="size-3.5" />回滚版本
                  </DropdownMenuItem>
                  <DropdownMenuSeparator />
                  <DropdownMenuGroup>
                    <DropdownMenuLabel className="text-[11px] text-muted-foreground">
                      诊断（平台会定期自动执行）
                    </DropdownMenuLabel>
                    <DropdownMenuItem onClick={() => void runSourceAction("sync")}>
                      <RefreshCw className="size-3.5" />立即同步来源
                    </DropdownMenuItem>
                    <DropdownMenuItem onClick={() => void runSourceAction("conformance")}>
                      <ShieldCheck className="size-3.5" />契约检查
                    </DropdownMenuItem>
                    <DropdownMenuItem onClick={() => void runSourceAction("health")}>
                      <Activity className="size-3.5" />健康检查
                    </DropdownMenuItem>
                  </DropdownMenuGroup>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem
                    variant="destructive"
                    disabled={governance.lifecycle_status === "retired"}
                    onClick={() => void stopOrRetire("stop")}
                  >
                    紧急停止
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    variant="destructive"
                    disabled={governance.lifecycle_status === "retired"}
                    onClick={() => void stopOrRetire("retire")}
                  >
                    退役智能体
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          </div>
          <dl className="grid gap-x-5 gap-y-2 border-t border-border pt-3 text-xs sm:grid-cols-[120px_1fr]">
            <dt className="text-muted-foreground">来源</dt>
            <dd className="break-all">
              {governance.source_provider} · {governance.external_agent_id} · 来源 v{governance.source_version}
            </dd>
            <dt className="text-muted-foreground">版本</dt>
            <dd>
              本地 revision {response.current_revision}
              {governance.published_revision != null && ` · 已发布 revision ${governance.published_revision}`}
            </dd>
            <dt className="text-muted-foreground">发布有效期</dt>
            <dd>
              {governance.review_due_at != null
                ? `截至 ${new Date(governance.review_due_at).toLocaleString()}`
                : "下次发布后开始计算"}
            </dd>
            <dt className="text-muted-foreground">黄金回归</dt>
            <dd>
              <span
                className={
                  response.eval_gate.passed
                    ? "text-emerald-700 dark:text-emerald-400"
                    : "text-amber-700 dark:text-amber-400"
                }
              >
                {response.eval_gate.message}
              </span>
              {/* This panel only renders for imported/federated agents
                  (governance row required). Their eval runs execute against
                  the platform's own model with a synthetic placeholder
                  prompt, not the external agent's real runtime — so a
                  pass/fail here doesn't reflect remote behavior. */}
              <p className="mt-1 text-amber-700 dark:text-amber-400">
                该智能体为外部联邦来源，评估结果仅供参考，不反映远端真实行为。
              </p>
            </dd>
            <dt className="text-muted-foreground">运行凭据</dt>
            <dd>
              {governance.credential_scope === "personal" ? (
                "属主隔离凭据（共享模式）"
              ) : (
                <span className="inline-flex flex-wrap items-center gap-2">
                  BYO：每个使用者需配置自己的密钥
                  {byoConfigured === true && <Badge variant="outline">你已配置</Badge>}
                  {byoConfigured === false && <Badge variant="destructive">你未配置</Badge>}
                  <button
                    type="button"
                    onClick={() => setByoDialogOpen(true)}
                    className="underline underline-offset-2 text-muted-foreground hover:text-foreground"
                  >
                    {byoConfigured ? "更新我的密钥" : "配置我的密钥"}
                  </button>
                </span>
              )}
            </dd>
            {RUNTIME_MAPPING_PROVIDERS.has(governance.source_provider) && (
              <>
                <dt className="text-muted-foreground">执行映射</dt>
                <dd>
                  <span className="inline-flex flex-wrap items-center gap-2">
                    该来源需要人工确认请求/响应字段映射后才能运行会话
                    <button
                      type="button"
                      onClick={() => void openMappingDialog()}
                      className="underline underline-offset-2 text-muted-foreground hover:text-foreground"
                    >
                      配置映射
                    </button>
                  </span>
                </dd>
              </>
            )}
          </dl>
        </div>
      </Card>
      {lastReport && (
        <Card className="mt-3 p-4">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-semibold">
              最近{lastReport.kind}详情
              <span className={`ml-2 rounded px-2 py-0.5 text-xs font-medium ${lastReport.report.can_activate ? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-400" : "bg-destructive/10 text-destructive"}`}>
                {lastReport.report.can_activate ? "通过" : "存在阻断项"}
              </span>
            </h3>
            <button
              type="button"
              onClick={() => setLastReport(null)}
              className="text-xs text-muted-foreground hover:text-foreground"
              aria-label="收起检查详情"
            >
              收起
            </button>
          </div>
          <ul className="mt-3 grid gap-1.5">
            {lastReport.report.checks.map((check, index) => {
              const meta = PREFLIGHT_VERDICT_META[check.verdict] ?? PREFLIGHT_VERDICT_META.unverified;
              return (
                <li key={`${check.id}-${index}`} className="flex items-start gap-2 text-xs">
                  <span className={`mt-0.5 shrink-0 rounded px-1.5 py-0.5 font-medium ${meta.className}`}>
                    {meta.label}
                  </span>
                  <span>
                    <span className="font-medium">{check.label}</span>
                    <span className="ml-1 text-muted-foreground">{check.detail}</span>
                  </span>
                </li>
              );
            })}
          </ul>
        </Card>
      )}
      {source && (
        <Card className="mt-3 overflow-hidden">
          <div className="flex flex-wrap items-start justify-between gap-3 border-b border-border px-4 py-3">
            <div>
              <h3 className="text-sm font-semibold">来源、漂移与运行保障</h3>
              <p className="mt-1 text-xs text-muted-foreground">
                {MANAGEMENT_MODE_LABELS[source.source.management_mode] ?? source.source.management_mode} ·{" "}
                {SYNC_STATE_LABELS[source.source.sync_state] ?? source.source.sync_state} · 来源快照 v
                {source.current_snapshot?.version ?? "-"}
              </p>
            </div>
            {busy !== null && ["sync", "conformance", "health"].includes(busy) && (
              <span className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
                <RefreshCw className="size-3.5 animate-spin motion-reduce:animate-none" />
                执行中…
              </span>
            )}
          </div>
          <div className="grid gap-4 p-4 lg:grid-cols-2">
            <div className="grid gap-2 text-xs">
              <div className="flex items-center justify-between rounded-md border border-border px-3 py-2">
                <span className="text-muted-foreground">连接器</span>
                <span className="font-mono">{source.source.connector_id ?? "平台托管来源"}</span>
              </div>
              <div className="flex items-center justify-between rounded-md border border-border px-3 py-2">
                <span className="text-muted-foreground">运行时契约</span>
                <span>
                  {source.conformance
                    ? `${source.conformance.contract_version} · ${CONFORMANCE_STATUS_LABELS[source.conformance.status] ?? source.conformance.status}`
                    : "尚未检查"}
                </span>
              </div>
              <div className="flex items-center justify-between rounded-md border border-border px-3 py-2">
                <span className="text-muted-foreground">规范化问题</span>
                <span>{source.current_snapshot?.normalization_issues.length ?? 0} 项</span>
              </div>
              <div className="flex items-center justify-between rounded-md border border-border px-3 py-2">
                <span className="text-muted-foreground">最近健康记录</span>
                <span>
                  {source.recent_health_checks.length} 项
                  {health && (
                    <span className={health.healthy ? "ml-1.5 text-emerald-600 dark:text-emerald-400" : "ml-1.5 text-destructive"}>
                      · 最新{health.healthy ? "健康" : "异常"}（{health.origin === "manual" ? "手动" : "自动"}，{relativeTimeLabel(health.checkedAt)}）
                    </span>
                  )}
                </span>
              </div>
              {source.conformance && source.conformance.checks.length > 0 && (
                <div className="rounded-md border border-border px-3 py-2">
                  <span className="text-muted-foreground">契约检查明细</span>
                  <ul className="mt-1.5 grid gap-1">
                    {source.conformance.checks.map((check) => (
                      <li key={check.id} className="flex items-start gap-2">
                        <span className={`mt-0.5 shrink-0 rounded px-1.5 py-0.5 font-medium ${check.passed ? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-400" : check.required ? "bg-destructive/10 text-destructive" : "bg-amber-500/10 text-amber-700 dark:text-amber-400"}`}>
                          {check.passed ? "通过" : check.required ? "未通过" : "可选未通过"}
                        </span>
                        <span className="text-muted-foreground">{check.detail}</span>
                      </li>
                    ))}
                  </ul>
                </div>
              )}
            </div>
            <div id="drift-review">
              <h4 className="text-xs font-semibold">漂移发现</h4>
              {source.drift_findings.filter((finding) => finding.resolution === "open").length === 0 ? (
                <p className="mt-2 rounded-md border border-dashed border-border px-3 py-4 text-xs text-muted-foreground">没有待处理的来源漂移。</p>
              ) : (
                <div className="mt-2 grid max-h-44 gap-1.5 overflow-y-auto">
                  {source.drift_findings.filter((finding) => finding.resolution === "open").map((finding) => (
                    <div key={finding.id} className="rounded-md border border-border px-3 py-2 text-xs">
                      <div className="flex items-center justify-between gap-3">
                        <span className="truncate font-mono">{finding.field_path}</span>
                        <Badge variant={finding.risk === "critical" || finding.risk === "high" ? "destructive" : "outline"}>
                          {DRIFT_RISK_LABELS[finding.risk] ?? finding.risk}
                        </Badge>
                      </div>
                      <div className="mt-1 truncate text-muted-foreground" title={`${driftValuePreview(finding.previous_value)} → ${driftValuePreview(finding.candidate_value)}`}>
                        {driftValuePreview(finding.previous_value)} → {driftValuePreview(finding.candidate_value)}
                      </div>
                    </div>
                  ))}
                </div>
              )}
              {source.candidate_snapshot && (
                <div className="mt-3">
                  <Button size="sm" disabled={busy !== null} onClick={() => setDriftDialogOpen(true)}>
                    评审变更
                  </Button>
                </div>
              )}
            </div>
          </div>
        </Card>
      )}
      {/* Emergency stop / retire live in the "更多" menu on the status card,
          which renders regardless of the source-overview fetch — safety
          controls never depend on an unrelated request. */}
      {error && <p className="mt-3 rounded-md border border-destructive/30 bg-destructive/10 px-4 py-2 text-xs text-destructive">{error}</p>}

      <Dialog
        open={driftDialogOpen}
        onOpenChange={(open) => {
          setDriftDialogOpen(open);
          // Escape/backdrop dismissal is "I don't want to decide yet", not a
          // decision — clear the draft reason so it never leaks into the next
          // time this dialog opens (this agent later, or a different one).
          if (!open) setDriftReason("");
        }}
      >
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>评审来源变更</DialogTitle>
            <DialogDescription>
              远端定义与当前运行版本存在以下差异。接受后配置将更新并回到草稿状态，需重新走检查与发布流程；拒绝则继续运行当前版本。
            </DialogDescription>
          </DialogHeader>
          <div className="grid max-h-[50vh] gap-2 overflow-y-auto py-1">
            {(source?.drift_findings ?? [])
              .filter((finding) => finding.resolution === "open")
              .map((finding) => (
                <div key={finding.id} className="rounded-md border border-border p-3 text-xs">
                  <div className="flex items-center justify-between gap-3">
                    <span className="font-mono font-medium">{finding.field_path}</span>
                    <Badge variant={finding.risk === "critical" || finding.risk === "high" ? "destructive" : "outline"}>
                      {DRIFT_RISK_LABELS[finding.risk] ?? finding.risk}风险
                    </Badge>
                  </div>
                  <div className="mt-2 grid gap-2 sm:grid-cols-2">
                    <div>
                      <p className="mb-1 text-[11px] text-muted-foreground">当前版本</p>
                      <pre className="max-h-32 overflow-auto whitespace-pre-wrap break-all rounded bg-muted/50 p-2 font-mono text-[11px]">{driftValueFull(finding.previous_value)}</pre>
                    </div>
                    <div>
                      <p className="mb-1 text-[11px] text-muted-foreground">远端候选</p>
                      <pre className="max-h-32 overflow-auto whitespace-pre-wrap break-all rounded bg-muted/50 p-2 font-mono text-[11px]">{driftValueFull(finding.candidate_value)}</pre>
                    </div>
                  </div>
                </div>
              ))}
            {(source?.drift_findings ?? []).filter((finding) => finding.resolution === "open").length === 0 && (
              <p className="rounded-md border border-dashed border-border px-3 py-4 text-center text-xs text-muted-foreground">
                没有字段级差异明细，可在下方直接决定是否采纳候选快照。
              </p>
            )}
          </div>
          <div className="grid gap-1.5">
            <Label htmlFor="drift-reason" className="text-xs">
              决定原因（可选，写入审计记录）
            </Label>
            <Textarea
              id="drift-reason"
              rows={2}
              value={driftReason}
              onChange={(event) => setDriftReason(event.target.value)}
              placeholder="例如：远端只更新了提示词措辞，已确认无风险。"
            />
          </div>
          <DialogFooter>
            <Button variant="outline" size="sm" disabled={busy !== null} onClick={() => setDriftDialogOpen(false)}>
              取消
            </Button>
            <Button variant="outline" size="sm" className="text-destructive" disabled={busy !== null} onClick={() => void runSourceAction("reject")}>
              {busy === "reject" ? "处理中..." : "拒绝变更"}
            </Button>
            <Button size="sm" disabled={busy !== null} onClick={() => void runSourceAction("accept")}>
              {busy === "accept" ? "处理中..." : "接受并重新发布"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={byoDialogOpen}
        onOpenChange={(open) => {
          setByoDialogOpen(open);
          if (!open) setByoKeyDraft("");
        }}
      >
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>{byoConfigured ? "更新我的密钥" : "配置我的密钥"}</DialogTitle>
            <DialogDescription>
              该智能体使用 BYO 凭据模式：密钥只对你自己的会话生效，加密存储，其他用户需各自配置。
            </DialogDescription>
          </DialogHeader>
          <div className="grid gap-1.5 py-1">
            <Label htmlFor="byo-key" className="text-xs">
              API Key
            </Label>
            <Input
              id="byo-key"
              type="password"
              autoComplete="new-password"
              value={byoKeyDraft}
              onChange={(event) => setByoKeyDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") void saveByoKey();
              }}
            />
          </div>
          <DialogFooter>
            <Button variant="outline" size="sm" onClick={() => setByoDialogOpen(false)}>
              取消
            </Button>
            <Button size="sm" disabled={!byoKeyDraft.trim()} onClick={() => void saveByoKey()}>
              保存
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={mappingDialogOpen} onOpenChange={setMappingDialogOpen}>
        {/* Width follows content, not just the probe: route lists and field
            pickers are already cramped at max-w-sm. */}
        <DialogContent
          className={
            probe || mappingPaths.length > 1 || inputCandidates.length > 0
              ? "max-w-2xl"
              : "max-w-sm"
          }
        >
          <DialogHeader>
            <DialogTitle>配置执行映射</DialogTitle>
            <DialogDescription>
              {governance.source_provider === "openapi"
                ? "告诉平台调用哪个站内路径、以及请求/响应里提示词和答案分别放在哪个字段。"
                : "告诉平台请求/响应里提示词和答案分别放在哪个字段；留空则使用该运行时的默认字段名。"}
            </DialogDescription>
          </DialogHeader>
          {mappingSuggestLoading && (
            <p className="text-xs text-muted-foreground">正在尝试自动获取输入/输出结构…</p>
          )}
          {!mappingSuggestLoading && mappingSuggestNote && (
            <p className="text-xs text-amber-700 dark:text-amber-400">{mappingSuggestNote}</p>
          )}
          {!mappingSuggestLoading && !mappingSuggestNote && (mappingDraft.input_field || mappingDraft.output_path) && (
            <p className="text-xs text-emerald-700 dark:text-emerald-400">
              已根据来源的 schema 接口自动填入建议值，可直接确认或按需修改。
            </p>
          )}
          <div className="grid gap-3 py-1">
            {governance.source_provider === "openapi" && (
              <div className="grid gap-1.5">
                <Label htmlFor="mapping-path" className="text-xs">
                  站内路径（必填，如 /agents/run）
                </Label>
                {mappingPaths.length > 1 && (
                  <div className="max-h-32 overflow-auto rounded border border-border bg-muted/40 p-1">
                    {mappingPaths.map((candidate) => {
                      const selected = (mappingDraft.path ?? "") === candidate.path;
                      return (
                        <button
                          key={candidate.path}
                          type="button"
                          onClick={() => applyPathSuggestion(candidate)}
                          className={`flex w-full items-baseline gap-2 rounded px-1 py-0.5 text-left hover:bg-muted ${
                            selected ? "bg-primary/10" : ""
                          }`}
                        >
                          <span className="shrink-0 font-mono text-[11px]">{candidate.path}</span>
                          {candidate.summary && (
                            <span className="truncate text-[11px] text-muted-foreground">
                              {candidate.summary}
                            </span>
                          )}
                        </button>
                      );
                    })}
                  </div>
                )}
                <Input
                  id="mapping-path"
                  value={mappingDraft.path ?? ""}
                  onChange={(event) =>
                    setMappingDraft((draft) => ({ ...draft, path: event.target.value }))
                  }
                  placeholder="/agents/run"
                />
                {mappingPaths.length > 0 && (
                  <p className="text-[11px] text-muted-foreground">
                    {mappingPaths.length === 1
                      ? "已自动填入来源规范中唯一的 POST 路由。"
                      : `来源规范声明了 ${mappingPaths.length} 个 POST 路由，点选可一并填入字段建议。`}
                  </p>
                )}
              </div>
            )}
            <div className="grid gap-1.5">
              <Label htmlFor="mapping-input-field" className="flex items-center gap-1.5 text-xs">
                请求字段（默认 {governance.source_provider === "crewai" ? "topic" : "input"}）
                <OriginBadge origin={mappingOrigins.input_field} />
              </Label>
              <SchemaFieldPicker
                candidates={inputCandidates}
                selected={mappingDraft.input_field?.trim() ?? ""}
                // Picking from the schema is still the source's *claim*, not an
                // observation — same provenance as the auto-fill.
                onSelect={(name) => setMappingField("input_field", name, "spec")}
              />
              <Input
                id="mapping-input-field"
                value={mappingDraft.input_field ?? ""}
                onChange={(event) =>
                  setMappingField("input_field", event.target.value, "manual")
                }
                placeholder={governance.source_provider === "crewai" ? "topic" : "input"}
              />
              {/* Say *why* nothing was filled in. A blank field otherwise looks
                  identical whether the platform looked and could not decide or
                  never looked at all. */}
              {!mappingDraft.input_field?.trim() && inputCandidates.length > 1 && (
                <p className="text-[11px] text-amber-700 dark:text-amber-400">
                  来源声明了 {inputCandidates.length} 个字段，无法判断哪个承载用户输入，请选择。
                </p>
              )}
              {isFieldUndeclared(mappingDraft.input_schema, mappingDraft.input_field) && (
                <p className="text-[11px] text-amber-700 dark:text-amber-400">
                  该字段不在来源声明的请求字段中。仍可保存——规范可能不完整，试跑结果才是依据。
                </p>
              )}
            </div>
            {governance.source_provider === "openapi" ? (
              <div className="grid gap-1.5">
                <Label htmlFor="mapping-output-field" className="flex items-center gap-1.5 text-xs">
                  响应字段（默认 output）
                  <OriginBadge origin={mappingOrigins.output_field} />
                </Label>
                <SchemaFieldPicker
                  candidates={outputCandidates}
                  selected={mappingDraft.output_field?.trim() ?? ""}
                  onSelect={(name) => setMappingField("output_field", name, "spec")}
                />
                <Input
                  id="mapping-output-field"
                  value={mappingDraft.output_field ?? ""}
                  onChange={(event) =>
                    setMappingField("output_field", event.target.value, "manual")
                  }
                  placeholder="output"
                />
                {!mappingDraft.output_field?.trim() && outputCandidates.length > 1 && (
                  <p className="text-[11px] text-amber-700 dark:text-amber-400">
                    来源声明了 {outputCandidates.length} 个响应字段，无法判断哪个是答案，请选择。
                  </p>
                )}
                {/* Catches the mistake that otherwise only surfaces as a failed
                    session: "response did not contain mapped field X". */}
                {isFieldUndeclared(mappingDraft.output_schema, mappingDraft.output_field) && (
                  <p className="text-[11px] text-amber-700 dark:text-amber-400">
                    该字段不在来源声明的响应字段中（可选：
                    {outputCandidates.map((candidate) => candidate.name).join("、")}）。
                    仍可保存——规范可能不完整，试跑结果才是依据。
                  </p>
                )}
              </div>
            ) : (
              <div className="grid gap-1.5">
                <Label htmlFor="mapping-output-path" className="flex items-center gap-1.5 text-xs">
                  响应字段路径（默认 {governance.source_provider === "crewai" ? "/result" : "/output"}）
                  <OriginBadge origin={mappingOrigins.output_path} />
                </Label>
                <Input
                  id="mapping-output-path"
                  value={mappingDraft.output_path ?? ""}
                  onChange={(event) =>
                    setMappingField("output_path", event.target.value, "manual")
                  }
                  placeholder={governance.source_provider === "crewai" ? "/result" : "/output"}
                />
              </div>
            )}
            {PROBE_PROVIDERS.has(governance.source_provider) && (
              <div className="grid gap-2 rounded-md border border-border p-2.5">
                <div className="flex items-center justify-between gap-2">
                  <span className="text-xs text-muted-foreground">
                    不确定答案在哪个字段？试跑一次，按真实响应点选。
                  </span>
                  <Button
                    variant="outline"
                    size="sm"
                    // OpenAPI has no default route to probe, so the path must
                    // be filled in before there is anything to call.
                    disabled={
                      probeRunning ||
                      (governance.source_provider === "openapi" && !mappingDraft.path?.trim())
                    }
                    onClick={() => void runProbe()}
                  >
                    <RefreshCw className="size-3.5" />
                    {probeRunning ? "试跑中…" : "试跑"}
                  </Button>
                </div>
                <p className="text-[11px] text-muted-foreground">
                  试跑会真实调用一次该来源，可能产生副作用与模型开销。
                  {governance.source_provider === "openapi" && " 需先填写站内路径。"}
                </p>
                {probeError && (
                  <p className="text-xs text-destructive">{probeError}</p>
                )}
                {probe && (
                  <>
                    {probe.sentinel_paths.length > 0 ? (
                      <p className="text-xs text-emerald-700 dark:text-emerald-400">
                        请求字段 <code>{probe.input_field}</code> 已被读取，
                        输入回显于 <code>{probe.sentinel_paths[0]}</code>。
                      </p>
                    ) : (
                      <p className="text-xs text-amber-700 dark:text-amber-400">
                        响应中未找到本次输入，说明请求字段 <code>{probe.input_field}</code>{" "}
                        可能未被该来源读取——请先确认请求字段，否则它会静默收到空输入。
                      </p>
                    )}
                    <div className="max-h-64 overflow-auto rounded border border-border bg-muted/40 p-1 font-mono text-[11px]">
                      {flattenJsonPointers(probe.response).rows.map((row) => {
                        // OpenAPI reads its answer with a top-level field name,
                        // not a pointer, so only depth-1 rows are addressable
                        // there and the leading "/" is dropped.
                        const openapi = governance.source_provider === "openapi";
                        const selectable = !openapi || row.depth === 1;
                        const selected = openapi
                          ? (mappingDraft.output_field ?? "") === row.label
                          : (mappingDraft.output_path ?? "") === row.pointer;
                        const echoed = probe.sentinel_paths.includes(row.pointer);
                        return (
                          <button
                            key={row.pointer || "(root)"}
                            type="button"
                            disabled={!selectable}
                            onClick={() =>
                              openapi
                                ? setMappingField("output_field", row.label, "probe")
                                : setMappingField("output_path", row.pointer, "probe")
                            }
                            title={
                              selectable
                                ? row.pointer || "（整个响应）"
                                : "OpenAPI 只能映射顶层字段"
                            }
                            className={`flex w-full items-baseline gap-2 rounded px-1 py-0.5 text-left ${
                              selectable ? "hover:bg-muted" : "cursor-default opacity-50"
                            } ${selected ? "bg-primary/10 text-foreground" : ""}`}
                            style={{ paddingLeft: `${row.depth * 12 + 4}px` }}
                          >
                            <span className="shrink-0 text-muted-foreground">
                              {row.label || "根"}
                            </span>
                            <span className="truncate text-foreground/70">{row.preview}</span>
                            {echoed && (
                              <span className="ml-auto shrink-0 text-amber-700 dark:text-amber-400">
                                输入回显
                              </span>
                            )}
                          </button>
                        );
                      })}
                    </div>
                    <p className="text-[11px] text-muted-foreground">
                      {governance.source_provider === "openapi"
                        ? "点选顶层字段填入响应字段名。"
                        : "点选任意节点填入响应字段路径。选中数组本身也是合法映射（如 /messages）。"}
                      平台无法判断哪个字段可以对外展示，请确认所选字段不含内部推理或检索原文。
                    </p>
                  </>
                )}
              </div>
            )}
          </div>
          {/* States plainly what is about to be signed. Confirming a mapping
              assembled from the source's claims is a different act from
              confirming one that was watched working, and only the operator
              can decide whether that is good enough here. */}
          {unconfirmedFields.length > 0 && (
            <p className="text-[11px] text-amber-700 dark:text-amber-400">
              {unconfirmedFields.map((field) => FIELD_LABELS[field]).join("、")}
              尚未经过试跑验证，保存即表示你确认其取值正确。
            </p>
          )}
          <DialogFooter>
            <Button variant="outline" size="sm" onClick={() => setMappingDialogOpen(false)}>
              取消
            </Button>
            <Button
              size="sm"
              disabled={
                mappingSaving ||
                (governance.source_provider === "openapi" && !mappingDraft.path?.trim())
              }
              onClick={() => void saveMapping()}
            >
              {mappingSaving ? "保存中…" : "保存并确认"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </section>
  );
}

export function DraftPreflightPanel({
  agentId,
  initialReport,
  onReport,
  onActivated,
}: {
  agentId: string;
  initialReport: AgentPreflightReport | null;
  onReport: (report: AgentPreflightReport) => void;
  onActivated: () => void;
}) {
  const [report, setReport] = useState<AgentPreflightReport | null>(initialReport);
  const [checking, setChecking] = useState(false);
  const [activating, setActivating] = useState(false);
  const [panelError, setPanelError] = useState<string | null>(null);

  const refresh = async () => {
    setChecking(true);
    setPanelError(null);
    try {
      const nextReport = await preflightAgent(agentId);
      setReport(nextReport);
      onReport(nextReport);
    } catch (e) {
      setPanelError(apiErrorMessage(e, "预检失败"));
    } finally {
      setChecking(false);
    }
  };

  useEffect(() => {
    if (initialReport) {
      setReport(initialReport);
    } else {
      void refresh();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agentId, initialReport]);

  const activate = async () => {
    setActivating(true);
    setPanelError(null);
    try {
      await activateAgent(agentId);
      toast.success("智能体已激活");
      onActivated();
    } catch (e) {
      setPanelError(apiErrorMessage(e, "激活失败"));
    } finally {
      setActivating(false);
    }
  };

  return (
    <Card className="border-amber-500/40 p-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-sm font-semibold">
            草稿状态
            <span className="ml-2 rounded bg-amber-500/10 px-2 py-0.5 text-xs font-medium text-amber-700 dark:text-amber-400">
              未激活
            </span>
          </h2>
          <p className="mt-1 text-xs text-muted-foreground">
            草稿智能体可编辑、可在对话中测试，但不能手动运行或被定时任务触发。通过预检后即可激活。
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" disabled={checking} onClick={() => void refresh()}>
            {checking ? "检查中..." : "重新预检"}
          </Button>
          <Button
            size="sm"
            disabled={activating || !report?.can_activate}
            onClick={() => void activate()}
          >
            {activating ? "激活中..." : "激活"}
          </Button>
        </div>
      </div>
      {panelError && (
        <p className="mt-3 rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive">{panelError}</p>
      )}
      {report && (
        <ul className="mt-3 grid gap-1.5">
          {report.checks.map((check, index) => {
            const meta = PREFLIGHT_VERDICT_META[check.verdict] ?? PREFLIGHT_VERDICT_META.unverified;
            return (
              <li key={`${check.id}-${index}`} className="flex items-start gap-2 text-xs">
                <span className={`mt-0.5 shrink-0 rounded px-1.5 py-0.5 font-medium ${meta.className}`}>
                  {meta.label}
                </span>
                <span>
                  <span className="font-medium">{check.label}</span>
                  <span className="ml-1 text-muted-foreground">{check.detail}</span>
                </span>
              </li>
            );
          })}
        </ul>
      )}
    </Card>
  );
}
