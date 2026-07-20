"use client";

import { KeyRound, Lock, ShieldCheck } from "lucide-react";
import { AccessControlBrandIcon } from "@/components/brand-kit-icons";
import { ApiKeysPanel } from "@/components/api-keys-dialog";
import { Sidebar } from "@/components/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";

export default function KeysPage() {
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
              <span className="text-xs text-muted-foreground font-medium">/ 密钥管理</span>
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
                      <Lock className="size-3" /> 访问控制与授权
                    </span>
                    <span className="text-xs text-muted-foreground font-medium">零信任密钥管理</span>
                  </div>
                  <h1 className="text-xl font-bold tracking-tight text-foreground">
                    网关 API 密钥与调起授权
                  </h1>
                  <p className="text-xs text-muted-foreground leading-relaxed">
                    管理与签发外部应用访问网关的认证 Key。支持配置使用限额、有效期与绑定调起人。
                  </p>
                </div>
              </div>
            </div>

            <ApiKeysPanel />
          </div>
        </main>
      </div>
    </div>
  );
}
