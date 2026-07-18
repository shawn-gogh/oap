"use client";

import { useState, type Dispatch, type SetStateAction } from "react";
import { Check, Clipboard, Info, MessageSquare } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { connectMattermost } from "@/lib/api";
import type { Agent } from "@/lib/types";
import { cn } from "@/lib/utils";

export interface MattermostConfig {
  status?: string;
  server_url?: string;
  bot_user_id?: string;
  notification_channel_id?: string;
}

interface MattermostForm {
  serverUrl: string;
  botToken: string;
  webhookToken: string;
  notificationChannelId: string;
}

function originForMattermost() {
  if (typeof window === "undefined") return "http://localhost:3210";
  return window.location.origin;
}

function endpointFor(agentId: string) {
  return `${originForMattermost()}/api/agents/${encodeURIComponent(agentId)}/mattermost/events`;
}

export function mattermostConfig(ag: Agent | null): MattermostConfig {
  const config = (ag?.config ?? {}) as { mattermost?: MattermostConfig };
  return config.mattermost ?? {};
}

export function mattermostActionLabel(config: MattermostConfig) {
  if (config.status === "connected") return "Mattermost connected";
  return "Connect Mattermost";
}

export function mattermostActionClass(config: MattermostConfig) {
  if (config.status === "connected") {
    return "border-blue-500/35 bg-blue-500/10 text-blue-700 hover:bg-blue-500/15 dark:text-blue-300";
  }
  return "";
}

export function useMattermostAppFlow(setAgents: Dispatch<SetStateAction<Agent[] | null>>) {
  const [open, setOpen] = useState(false);
  const [agent, setAgent] = useState<Agent | null>(null);
  const [form, setForm] = useState<MattermostForm>({
    serverUrl: "",
    botToken: "",
    webhookToken: "",
    notificationChannelId: "",
  });
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const openMattermost = (ag: Agent) => {
    const existing = mattermostConfig(ag);
    setAgent(ag);
    setForm({
      serverUrl: existing.server_url || "",
      botToken: "",
      webhookToken: "",
      notificationChannelId: existing.notification_channel_id || "",
    });
    setError(null);
    setCopied(false);
    setOpen(true);
  };

  const copyEndpoint = async (value: string) => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
    } catch {
      setError("Could not copy. Select and copy the value from the field instead.");
    }
  };

  const existing: MattermostConfig = agent ? mattermostConfig(agent) : {};
  const connected = existing.status === "connected";
  const canSave =
    Boolean(agent) &&
    form.serverUrl.trim().length > 0 &&
    (form.botToken.trim().length > 0 || connected) &&
    (form.webhookToken.trim().length > 0 || connected);

  const saveMattermost = async () => {
    if (!agent) return;
    const serverUrl = form.serverUrl.trim();
    if (!serverUrl) {
      setError("Enter the Mattermost server URL.");
      return;
    }
    if (!form.botToken.trim() && !connected) {
      setError("Paste the bot account's Personal Access Token.");
      return;
    }
    if (!form.webhookToken.trim() && !connected) {
      setError("Paste the Outgoing Webhook's verification token.");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const result = await connectMattermost(agent.id, {
        server_url: serverUrl,
        bot_token: form.botToken.trim(),
        webhook_token: form.webhookToken.trim(),
        notification_channel_id: form.notificationChannelId.trim(),
      });
      setAgent(result.agent);
      setAgents((prev) => prev?.map((a) => (a.id === result.agent.id ? result.agent : a)) ?? null);
      setForm((current) => ({ ...current, botToken: "", webhookToken: "" }));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  const endpoint = agent ? endpointFor(agent.id) : "";

  const dialog = (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="max-h-[92vh] w-[calc(100vw-2rem)] max-w-none gap-0 overflow-hidden p-0 sm:max-w-[900px]">
        <div className="grid min-h-[520px] grid-cols-1 md:grid-cols-[250px_minmax(0,1fr)]">
          <div className="border-b border-border bg-muted/30 p-7 md:border-b-0 md:border-r">
            <div className="flex items-center gap-3">
              <div className="flex size-11 items-center justify-center rounded-lg border border-border bg-background">
                <MessageSquare className="size-5 text-blue-600 dark:text-blue-300" />
              </div>
              <div>
                <DialogTitle className="text-xl font-semibold tracking-tight">Mattermost</DialogTitle>
                <p className="mt-1 text-xs text-muted-foreground">自托管双向会话</p>
              </div>
            </div>

            <div className="mt-8 grid gap-3">
              {[
                ["1", "创建 Bot 账号", "在 Mattermost 后台创建，生成 Personal Access Token"],
                ["2", "配置 Outgoing Webhook", "指向下方 Endpoint，记录校验 Token"],
                ["3", "粘贴凭证", "在这里填入服务器地址与两个 Token"],
              ].map(([n, title, detail]) => (
                <div
                  key={n}
                  className="grid grid-cols-[32px_1fr] gap-3 rounded-lg border border-transparent px-3 py-3"
                >
                  <div
                    className={cn(
                      "flex size-8 items-center justify-center rounded-full border text-sm font-medium",
                      connected
                        ? "border-primary bg-primary text-primary-foreground"
                        : "border-border bg-background text-muted-foreground",
                    )}
                  >
                    {connected ? <Check className="size-4" /> : n}
                  </div>
                  <div className="min-w-0">
                    <p className="text-sm font-medium">{title}</p>
                    <p className="text-xs leading-5 text-muted-foreground">{detail}</p>
                  </div>
                </div>
              ))}
            </div>
          </div>

          <div className="flex min-h-0 flex-col">
            <DialogHeader className="border-b border-border px-7 py-6">
              <p className="text-sm leading-6 text-muted-foreground">
                自托管 Mattermost 场景没有类似 Slack App 的公开注册流程——请先在你自己的
                Mattermost 后台手动创建一个 Bot 账号（生成 Personal Access Token），再为目标频道
                配置一个 Outgoing Webhook，指向下方 Endpoint，并记录其校验 Token。完成后把三项
                信息粘贴到这里即可开始双向对话。
              </p>
            </DialogHeader>

            {agent && (
              <div className="min-h-0 flex-1 overflow-y-auto px-7 py-6">
                <div className="grid gap-5">
                  <div className="grid gap-1.5">
                    <Label htmlFor="mattermost-endpoint">Outgoing Webhook Endpoint</Label>
                    <div className="flex gap-2">
                      <Input
                        id="mattermost-endpoint"
                        value={endpoint}
                        readOnly
                        className="font-mono text-xs"
                      />
                      <Button
                        type="button"
                        variant="outline"
                        size="icon"
                        onClick={() => copyEndpoint(endpoint)}
                        aria-label="Copy endpoint"
                      >
                        <Clipboard className="size-4" />
                      </Button>
                    </div>
                  </div>

                  <div className="grid gap-1.5">
                    <Label htmlFor="mattermost-server-url">服务器地址</Label>
                    <Input
                      id="mattermost-server-url"
                      value={form.serverUrl}
                      onChange={(e) => setForm((f) => ({ ...f, serverUrl: e.target.value }))}
                      placeholder="https://chat.example.com"
                      className="font-mono text-xs"
                    />
                  </div>

                  <div className="grid gap-1.5">
                    <Label htmlFor="mattermost-bot-token">Bot Personal Access Token</Label>
                    <Input
                      id="mattermost-bot-token"
                      type="password"
                      autoComplete="new-password"
                      value={form.botToken}
                      onChange={(e) => setForm((f) => ({ ...f, botToken: e.target.value }))}
                      placeholder={connected ? "留空则保留当前 Token" : "粘贴 Bot 账号的 Personal Access Token"}
                    />
                  </div>

                  <div className="grid gap-1.5">
                    <Label htmlFor="mattermost-webhook-token">Outgoing Webhook Token</Label>
                    <Input
                      id="mattermost-webhook-token"
                      type="password"
                      autoComplete="new-password"
                      value={form.webhookToken}
                      onChange={(e) => setForm((f) => ({ ...f, webhookToken: e.target.value }))}
                      placeholder={connected ? "留空则保留当前 Token" : "粘贴 Outgoing Webhook 的校验 Token"}
                    />
                  </div>

                  <div className="grid gap-1.5">
                    <Label htmlFor="mattermost-notification-channel">治理通知频道 ID</Label>
                    <Input
                      id="mattermost-notification-channel"
                      value={form.notificationChannelId}
                      onChange={(e) =>
                        setForm((f) => ({ ...f, notificationChannelId: e.target.value }))
                      }
                      placeholder="留空则不主动推送治理通知"
                      className="font-mono text-xs"
                    />
                    <p className="text-xs leading-5 text-muted-foreground">
                      配置后，发布审批、健康自动暂停和高风险漂移会主动推送到该频道。
                    </p>
                  </div>

                  {connected && (
                    <div className="grid gap-2 rounded-lg border border-border bg-muted/20 p-4">
                      <p className="text-xs font-medium uppercase text-muted-foreground">当前状态</p>
                      <p className="text-sm text-muted-foreground">
                        已连接 · Bot User ID: <span className="font-mono">{existing.bot_user_id}</span>
                      </p>
                    </div>
                  )}

                  {(copied || error) && (
                    <div
                      className={cn(
                        "flex items-center gap-2 rounded-md border px-3 py-2 text-xs",
                        error
                          ? "border-destructive/40 bg-destructive/10 text-destructive"
                          : "border-emerald-500/35 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300",
                      )}
                    >
                      <Info className="size-3.5" />
                      {error || "Endpoint copied"}
                    </div>
                  )}
                </div>
              </div>
            )}

            <DialogFooter className="m-0 border-t bg-background px-7 py-4">
              <Button onClick={saveMattermost} disabled={saving || !canSave}>
                {saving ? "连接中…" : "连接 Mattermost"}
              </Button>
            </DialogFooter>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );

  return { dialog, openMattermost };
}
