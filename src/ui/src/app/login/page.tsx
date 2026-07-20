"use client";

import { Suspense, useState, useEffect, FormEvent } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import {
  ShieldCheck,
  KeyRound,
  Eye,
  EyeOff,
  Loader2,
  Lock,
  Zap,
  ArrowRight,
  Terminal,
  Cpu,
  AlertCircle,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { OapLogo } from "@/components/oap-logo";
import {
  ApiError,
  clearStoredMasterKey,
  loginWithAccessKey,
  setStoredMasterKey,
} from "@/lib/api";

export default function LoginPage() {
  return (
    <Suspense fallback={<LoginSkeleton />}>
      <LoginForm />
    </Suspense>
  );
}

function LoginSkeleton() {
  return (
    <div className="min-h-dvh flex items-center justify-center bg-background p-4">
      <div className="size-8 animate-spin rounded-full border-2 border-primary border-t-transparent" />
    </div>
  );
}

function LoginForm() {
  const router = useRouter();
  const params = useSearchParams();
  const next = params.get("next") || "/sessions/";

  const [key, setKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    clearStoredMasterKey();
  }, []);

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    if (submitting || !key.trim()) return;
    setError(null);
    setSubmitting(true);
    try {
      await loginWithAccessKey(key.trim());
      setStoredMasterKey(key.trim());
      router.replace(next);
    } catch (e) {
      clearStoredMasterKey();
      const msg =
        e instanceof ApiError && e.status === 401
          ? "访问密钥无效或已被注销。"
          : e instanceof Error
            ? e.message
            : "登录认证失败，请检查网络或密钥。";
      setError(msg);
      setSubmitting(false);
    }
  }

  return (
    <div className="min-h-dvh flex w-full bg-background text-foreground overflow-hidden selection:bg-emerald-500/20">
      {/* Left Column: Pure Chinese Brand Showcase */}
      <div className="hidden lg:flex lg:w-1/2 relative flex-col justify-between p-12 border-r border-border/70 bg-card overflow-hidden">
        {/* Ambient Glow & Grid Matrix */}
        <div className="absolute left-0 top-0 -ml-24 -mt-24 size-96 rounded-full bg-emerald-500/10 blur-3xl pointer-events-none" />
        <div className="absolute right-0 bottom-0 -mr-24 -mb-24 size-96 rounded-full bg-blue-500/10 blur-3xl pointer-events-none" />
        <div className="absolute inset-0 bg-[radial-gradient(#e5e7eb_1px,transparent_1px)] dark:bg-[radial-gradient(#1f2937_1px,transparent_1px)] [background-size:24px_24px] opacity-40 pointer-events-none" />

        <div className="relative z-10">
          <OapLogo size={32} showText subtitle="控制平面 2.0" />
        </div>

        <div className="relative z-10 space-y-8 my-auto max-w-lg">
          <div className="space-y-3">
            <span className="inline-flex items-center gap-1.5 rounded-full border border-emerald-500/30 bg-emerald-500/10 px-3 py-1 text-xs font-medium text-emerald-600 dark:text-emerald-400">
              <ShieldCheck className="size-3.5" />
              企业级智能体控制平面
            </span>
            <h1 className="text-3xl font-bold tracking-tight text-foreground leading-tight">
              构建、编排与管控 <br />
              <span className="bg-gradient-to-r from-emerald-500 via-teal-400 to-blue-500 bg-clip-text text-transparent">
                下一代自主智能体集群
              </span>
            </h1>
            <p className="text-xs text-muted-foreground leading-relaxed">
              统一集成大语言模型网关、扩展工具生态、系统法则约束与凭证保险库。在零信任沙箱中实时调度智能体任务。
            </p>
          </div>

          {/* Feature Highlights Grid */}
          <div className="grid grid-cols-2 gap-3 pt-2">
            <div className="rounded-xl border border-border/70 bg-background/60 p-3.5 backdrop-blur shadow-2xs">
              <div className="flex items-center gap-2 text-xs font-semibold text-foreground">
                <Lock className="size-4 text-emerald-500" />
                <span>高强度加密保险库</span>
              </div>
              <p className="mt-1 text-[11px] text-muted-foreground leading-snug">
                密钥隔离存储，沙箱安全注入
              </p>
            </div>

            <div className="rounded-xl border border-border/70 bg-background/60 p-3.5 backdrop-blur shadow-2xs">
              <div className="flex items-center gap-2 text-xs font-semibold text-foreground">
                <Zap className="size-4 text-cyan-500" />
                <span>扩展工具协议枢纽</span>
              </div>
              <p className="mt-1 text-[11px] text-muted-foreground leading-snug">
                无缝连接工具链与外部数据源
              </p>
            </div>

            <div className="rounded-xl border border-border/70 bg-background/60 p-3.5 backdrop-blur shadow-2xs">
              <div className="flex items-center gap-2 text-xs font-semibold text-foreground">
                <Terminal className="size-4 text-blue-500" />
                <span>系统法则与提示规约</span>
              </div>
              <p className="mt-1 text-[11px] text-muted-foreground leading-snug">
                全局提示词规范与输出约束
              </p>
            </div>

            <div className="rounded-xl border border-border/70 bg-background/60 p-3.5 backdrop-blur shadow-2xs">
              <div className="flex items-center gap-2 text-xs font-semibold text-foreground">
                <Cpu className="size-4 text-teal-500" />
                <span>领域技能矩阵</span>
              </div>
              <p className="mt-1 text-[11px] text-muted-foreground leading-snug">
                标准操作流程与能力扩充
              </p>
            </div>
          </div>
        </div>

        <div className="relative z-10 flex items-center justify-between text-xs text-muted-foreground font-mono">
          <span>安全沙箱防护网络</span>
          <span>系统就绪 · 实时监控中</span>
        </div>
      </div>

      {/* Right Column: Authentication Form */}
      <div className="flex-1 flex flex-col justify-between p-6 sm:p-12 bg-background">
        <div className="flex justify-between items-center lg:hidden">
          <OapLogo size={26} showText />
        </div>

        <div className="w-full max-w-sm mx-auto my-auto space-y-6">
          <div className="space-y-1.5">
            <h2 className="text-2xl font-bold tracking-tight text-foreground">
              控制台登录
            </h2>
            <p className="text-xs text-muted-foreground leading-relaxed">
              输入你的管理员主访问密钥以验证身份并进入智能体控制中心。
            </p>
          </div>

          <form onSubmit={onSubmit} className="space-y-4">
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <Label htmlFor="key" className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
                  访问密钥
                </Label>
              </div>

              <div className="relative">
                <Input
                  id="key"
                  type={showKey ? "text" : "password"}
                  value={key}
                  onChange={(e) => setKey(e.target.value)}
                  placeholder="粘贴密钥，例如: sk-..."
                  className="pr-10 font-mono text-xs h-10 bg-card focus-visible:ring-emerald-500/30"
                  autoFocus
                  autoComplete="current-password"
                  spellCheck={false}
                  disabled={submitting}
                />
                <button
                  type="button"
                  onClick={() => setShowKey((s) => !s)}
                  className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-colors"
                  aria-label={showKey ? "隐藏密钥" : "显示密钥"}
                >
                  {showKey ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
                </button>
              </div>
            </div>

            {error && (
              <div className="rounded-xl border border-destructive/40 bg-destructive/10 p-3 text-xs text-destructive flex items-center gap-2">
                <AlertCircle className="size-4 shrink-0" />
                <span>{error}</span>
              </div>
            )}

            <Button
              type="submit"
              disabled={submitting || key.trim().length === 0}
              className="w-full h-10 bg-emerald-600 hover:bg-emerald-700 text-white font-medium gap-2 text-xs shadow-xs active:scale-[0.98] transition-all"
            >
              {submitting ? (
                <>
                  <Loader2 className="size-4 animate-spin" />
                  验证访问权限中…
                </>
              ) : (
                <>
                  进入控制台
                  <ArrowRight className="size-4" />
                </>
              )}
            </Button>
          </form>

          <div className="rounded-xl border border-border/60 bg-muted/30 p-3.5 text-xs text-muted-foreground space-y-1">
            <div className="flex items-center gap-1.5 font-medium text-foreground">
              <KeyRound className="size-3.5 text-emerald-500" />
              <span>密钥使用提示</span>
            </div>
            <p className="text-[11px] leading-relaxed">
              可以使用全局管理员主密钥，或在系统后台分发的个人访问密钥进行认证登录。
            </p>
          </div>
        </div>

        <div className="text-center text-[11px] font-mono text-muted-foreground py-2">
          开放智能体平台 &copy; 2026 版权所有
        </div>
      </div>
    </div>
  );
}
