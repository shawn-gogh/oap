"use client";

import { ServerCog, Zap, Cpu } from "lucide-react";
import { ProvidersPanel } from "@/components/providers-panel";
import { Sidebar } from "@/components/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";

export default function ProvidersPage() {
  return (
    <div className="flex h-screen bg-background text-foreground overflow-hidden selection:bg-blue-500/20">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        {/* Pure Chinese Header */}
        <header className="flex h-12 shrink-0 items-center justify-between border-b border-border/80 px-4 bg-background/80 backdrop-blur">
          <div className="flex items-center gap-2.5">
            <div className="flex size-7 items-center justify-center rounded-md bg-blue-500/10 text-blue-600 dark:text-blue-400 ring-1 ring-blue-500/20">
              <ServerCog className="size-4" />
            </div>
            <div className="flex items-center gap-2">
              <span className="text-sm font-semibold tracking-tight">LLM 提供方管理</span>
              <span className="text-xs text-muted-foreground font-medium">/ 提供方</span>
            </div>
          </div>
          <ThemeToggle />
        </header>

        <main id="main-content" className="flex-1 overflow-y-auto p-6 space-y-6">
          <div className="mx-auto w-full max-w-5xl space-y-6">
            {/* Command Banner - Pure Chinese */}
            <div className="relative overflow-hidden rounded-2xl border border-border/70 bg-card p-6 shadow-xs">
              <div className="absolute right-0 top-0 -mr-16 -mt-16 size-64 rounded-full bg-blue-500/5 blur-3xl pointer-events-none" />
              <div className="relative z-10 flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
                <div className="space-y-1.5 max-w-xl">
                  <div className="flex items-center gap-2">
                    <span className="inline-flex items-center gap-1 rounded-md bg-blue-500/10 px-2 py-0.5 text-xs font-medium text-blue-600 dark:text-blue-400 border border-blue-500/20">
                      <Cpu className="size-3" /> 模型网关与路由
                    </span>
                    <span className="text-xs text-muted-foreground font-medium">多提供方统一网关</span>
                  </div>
                  <h1 className="text-xl font-bold tracking-tight text-foreground">
                    大语言模型提供方与 ApiKey 接入
                  </h1>
                  <p className="text-xs text-muted-foreground leading-relaxed">
                    统一配置 OpenAI、Claude、DeepSeek 及各类开源/自托管 LLM 提供方的访问密钥与基础 API Base。
                  </p>
                </div>
              </div>
            </div>

            <ProvidersPanel />
          </div>
        </main>
      </div>
    </div>
  );
}
