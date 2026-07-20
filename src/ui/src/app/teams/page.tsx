"use client";

import { Users, ShieldCheck, Lock } from "lucide-react";
import { AccessControlBrandIcon } from "@/components/brand-kit-icons";
import { Sidebar } from "@/components/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";
import { Button } from "@/components/ui/button";

export default function TeamsPage() {
  return (
    <div className="flex h-screen bg-background text-foreground overflow-hidden selection:bg-emerald-500/20">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        {/* Anti-slop Pure Chinese Header */}
        <header className="flex h-12 shrink-0 items-center justify-between border-b border-border/80 px-4 bg-background/80 backdrop-blur">
          <div className="flex items-center gap-2.5">
            <div className="flex size-7 items-center justify-center rounded-md bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 ring-1 ring-emerald-500/20">
              <AccessControlBrandIcon size={16} />
            </div>
            <div className="flex items-center gap-2">
              <span className="text-sm font-semibold tracking-tight">访问控制中心</span>
              <span className="text-xs text-muted-foreground font-medium">/ 团队管理</span>
            </div>
          </div>
          <ThemeToggle />
        </header>

        <main id="main-content" className="flex-1 overflow-y-auto p-6 space-y-6">
          <div className="mx-auto flex max-w-5xl flex-col gap-6">
            {/* Command Banner - Pure Chinese */}
            <div className="relative overflow-hidden rounded-2xl border border-border/70 bg-card p-6 shadow-xs">
              <div className="absolute right-0 top-0 -mr-16 -mt-16 size-64 rounded-full bg-emerald-500/5 blur-3xl pointer-events-none" />
              <div className="relative z-10 flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
                <div className="space-y-1.5 max-w-xl">
                  <div className="flex items-center gap-2">
                    <span className="inline-flex items-center gap-1 rounded-md bg-emerald-500/10 px-2 py-0.5 text-xs font-medium text-emerald-600 dark:text-emerald-400 border border-emerald-500/20">
                      <Lock className="size-3" /> 组织架构与协同
                    </span>
                    <span className="text-xs text-muted-foreground font-medium">团队级预算与配额</span>
                  </div>
                  <h1 className="text-xl font-bold tracking-tight text-foreground">
                    团队空间与多租户隔离
                  </h1>
                  <p className="text-xs text-muted-foreground leading-relaxed">
                    按部门或项目划分团队空间，实现模型 Token 额度控制、智能体隔离及多租户审计。
                  </p>
                </div>
              </div>
            </div>

            <div className="rounded-2xl border border-dashed border-border/80 bg-card p-12 text-center shadow-2xs">
              <div className="mx-auto mb-3 flex size-12 items-center justify-center rounded-2xl bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/20">
                <Users className="size-6" />
              </div>
              <h2 className="text-base font-bold tracking-tight text-foreground">暂无团队数据</h2>
              <p className="mt-1.5 text-xs text-muted-foreground max-w-sm mx-auto leading-relaxed">
                多租户团队权限控制在当前版本中默认跟随企业用户组策略，无需单独手动配置。
              </p>
              <Button className="mt-4 text-xs font-medium bg-emerald-600 hover:bg-emerald-700 text-white" disabled>
                新建团队
              </Button>
            </div>
          </div>
        </main>
      </div>
    </div>
  );
}
