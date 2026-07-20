"use client";

import { FormEvent, useEffect, useRef, useState } from "react";
import { Pencil, Plus, Save, Trash2, Users, X, UserCheck } from "lucide-react";
import { AccessControlBrandIcon } from "@/components/brand-kit-icons";
import { Sidebar } from "@/components/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  createUser,
  deactivateUser,
  listUsers,
  updateUser,
  type ManagedUser,
} from "@/lib/api";

export default function UsersPage() {
  const [users, setUsers] = useState<ManagedUser[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [id, setId] = useState("");
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [busy, setBusy] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [profile, setProfile] = useState({ display_name: "", email: "" });
  const [transferTargets, setTransferTargets] = useState<Record<string, string>>({});
  const linkedUserHandled = useRef(false);

  const load = async () => {
    try {
      setUsers(await listUsers());
      setError(null);
    } catch (err) {
      setUsers([]);
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  useEffect(() => { void load(); }, []);

  useEffect(() => {
    if (linkedUserHandled.current || !users || typeof window === "undefined") return;
    const id = new URLSearchParams(window.location.search).get("user");
    const user = id ? users.find((item) => item.id === id) : undefined;
    if (!user) return;
    linkedUserHandled.current = true;
    setEditingId(user.id);
    setProfile({ display_name: user.display_name, email: user.email ?? "" });
  }, [users]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!id.trim() || !name.trim() || busy) return;
    setBusy(true);
    try {
      await createUser({ id: id.trim(), display_name: name.trim(), email: email.trim() || undefined });
      setId(""); setName(""); setEmail("");
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const toggle = async (user: ManagedUser) => {
    try {
      await updateUser(user.id, { status: "active" });
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const edit = (user: ManagedUser) => {
    setEditingId(user.id);
    setProfile({ display_name: user.display_name, email: user.email ?? "" });
  };

  const saveProfile = async (user: ManagedUser) => {
    if (!profile.display_name.trim() || busy) return;
    setBusy(true);
    try {
      await updateUser(user.id, {
        display_name: profile.display_name.trim(),
        email: profile.email.trim() || null,
      });
      setEditingId(null);
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const deactivate = async (user: ManagedUser) => {
    if (busy) return;
    const transferTo = transferTargets[user.id]?.trim();
    if (!window.confirm(`停用“${user.display_name}”后会撤销其登录会话、个人密钥、个人凭证和组成员关系。${transferTo ? `其智能体将转移给 ${transferTo}。` : "如其拥有智能体，系统会要求选择接收用户。"}`)) return;
    setBusy(true);
    try {
      await deactivateUser(user.id, transferTo || undefined);
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const forbidden = error?.startsWith("HTTP 403") ?? false;
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
              <span className="text-xs text-muted-foreground font-medium">/ 用户管理</span>
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
                      <UserCheck className="size-3" /> 用户身份与归属
                    </span>
                    <span className="text-xs text-muted-foreground font-medium">平台账户控制</span>
                  </div>
                  <h1 className="text-xl font-bold tracking-tight text-foreground">
                    智能体拥有者与平台用户管理
                  </h1>
                  <p className="text-xs text-muted-foreground leading-relaxed">
                    创建、编辑与注销平台账户。用户可拥有独立的智能体，并可分配个人或用户组访问授权。
                  </p>
                </div>
              </div>
            </div>

            {error && <div className="rounded-xl border border-destructive/40 bg-destructive/10 p-3.5 text-xs font-mono text-destructive">{forbidden ? "需要管理员权限。" : error}</div>}

            {!forbidden && (
              <form onSubmit={submit} className="grid gap-3 rounded-2xl border border-border/70 bg-card p-4 shadow-2xs sm:grid-cols-4 items-center">
                <Input value={id} onChange={(event) => setId(event.target.value)} placeholder="用户 ID" className="h-9 text-xs font-mono" />
                <Input value={name} onChange={(event) => setName(event.target.value)} placeholder="显示名称" className="h-9 text-xs" />
                <Input value={email} onChange={(event) => setEmail(event.target.value)} placeholder="邮箱（可选）" type="email" className="h-9 text-xs" />
                <Button type="submit" size="sm" disabled={busy || !id.trim() || !name.trim()} className="h-9 text-xs bg-emerald-600 hover:bg-emerald-700 text-white font-medium">
                  <Plus className="size-3.5" />
                  新建用户
                </Button>
              </form>
            )}

            {users === null ? (
              <p className="text-xs text-muted-foreground font-mono">正在加载用户列表…</p>
            ) : (
              <div className="overflow-hidden rounded-2xl border border-border/70 bg-card shadow-2xs">
                {users.map((user) => (
                  <div key={user.id} className="border-b border-border/60 px-5 py-4 last:border-0 hover:bg-muted/30 transition-colors">
                    <div className="flex items-center justify-between gap-3">
                      <div className="min-w-0">
                        <div className="font-semibold text-sm text-foreground">{user.display_name}</div>
                        <div className="truncate font-mono text-xs text-muted-foreground mt-0.5">{user.id}{user.email ? ` · ${user.email}` : ""}</div>
                      </div>
                      <div className="flex shrink-0 items-center gap-2">
                        <span className={`text-xs font-medium px-2 py-0.5 rounded-md border ${
                          user.status === "active"
                            ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
                            : "border-border bg-muted text-muted-foreground"
                        }`}>
                          {user.status === "active" ? "启用中" : "已停用"}
                        </span>
                        {user.status === "active" ? (
                          <>
                            <Button size="sm" variant="outline" className="h-8 text-xs gap-1" onClick={() => edit(user)} disabled={busy}>
                              <Pencil className="size-3.5" />
                              编辑
                            </Button>
                            <Button size="sm" variant="outline" className="h-8 text-xs gap-1 text-destructive hover:text-destructive border-destructive/30 hover:bg-destructive/10" onClick={() => void deactivate(user)} disabled={busy}>
                              <Trash2 className="size-3.5" />
                              停用并清理
                            </Button>
                          </>
                        ) : (
                          <Button size="sm" className="h-8 text-xs bg-emerald-600 hover:bg-emerald-700 text-white font-medium" onClick={() => void toggle(user)} disabled={busy}>
                            重新启用
                          </Button>
                        )}
                      </div>
                    </div>
                    {editingId === user.id && (
                      <div className="mt-3 grid gap-2 rounded-xl border border-border/80 bg-muted/30 p-3.5 sm:grid-cols-[1fr_1fr_auto] items-center">
                        <Input value={profile.display_name} onChange={(event) => setProfile((current) => ({ ...current, display_name: event.target.value }))} placeholder="显示名称" className="h-8 text-xs" />
                        <Input value={profile.email} onChange={(event) => setProfile((current) => ({ ...current, email: event.target.value }))} placeholder="邮箱（可留空）" type="email" className="h-8 text-xs" />
                        <div className="flex gap-2">
                          <Button size="sm" className="h-8 text-xs bg-emerald-600 hover:bg-emerald-700 text-white font-medium gap-1" onClick={() => void saveProfile(user)} disabled={busy || !profile.display_name.trim()}>
                            <Save className="size-3.5" />
                            保存
                          </Button>
                          <Button size="sm" variant="ghost" className="h-8 text-xs" onClick={() => setEditingId(null)} disabled={busy} aria-label="取消编辑">
                            <X className="size-3.5" />
                          </Button>
                        </div>
                      </div>
                    )}
                    {user.status === "active" && (
                      <div className="mt-2.5 flex flex-wrap items-center gap-2 text-xs text-muted-foreground font-mono">
                        <span>停用前转移智能体资产给：</span>
                        <select
                          value={transferTargets[user.id] ?? ""}
                          onChange={(event) => setTransferTargets((current) => ({ ...current, [user.id]: event.target.value }))}
                          className="h-8 max-w-[260px] rounded-lg border border-border bg-background px-2.5 text-xs text-foreground font-mono"
                        >
                          <option value="">未选择接收用户</option>
                          {users.filter((candidate) => candidate.id !== user.id && candidate.status === "active").map((candidate) => (
                            <option key={candidate.id} value={candidate.id}>
                              {candidate.display_name}（{candidate.id}）
                            </option>
                          ))}
                        </select>
                      </div>
                    )}
                  </div>
                ))}
              </div>
            )}
          </div>
        </main>
      </div>
    </div>
  );
}
