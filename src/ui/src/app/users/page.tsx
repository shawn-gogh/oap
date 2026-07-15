"use client";

import { FormEvent, useEffect, useRef, useState } from "react";
import { Pencil, Plus, Save, Trash2, Users, X } from "lucide-react";

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
    <div className="flex h-screen bg-background text-foreground">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-12 shrink-0 items-center justify-between border-b border-border px-4">
          <div className="flex items-center gap-2"><Users className="size-4 text-muted-foreground" /><h1 className="text-sm font-semibold">用户管理</h1></div>
          <ThemeToggle />
        </header>
        <main id="main-content" className="flex-1 overflow-y-auto">
          <div className="mx-auto flex max-w-4xl flex-col gap-5 px-4 py-6">
            <div><h2 className="text-lg font-semibold">用户管理</h2><p className="text-sm text-muted-foreground">用户拥有智能体，并可获得个人或用户组授权。</p></div>
            {error && <p className="rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{forbidden ? "需要管理员权限。" : error}</p>}
            {!forbidden && <form onSubmit={submit} className="grid gap-2 rounded-lg border border-border bg-card p-4 sm:grid-cols-4">
              <Input value={id} onChange={(event) => setId(event.target.value)} placeholder="用户 ID" />
              <Input value={name} onChange={(event) => setName(event.target.value)} placeholder="显示名称" />
              <Input value={email} onChange={(event) => setEmail(event.target.value)} placeholder="邮箱（可选）" type="email" />
              <Button type="submit" disabled={busy || !id.trim() || !name.trim()}><Plus className="size-4" />创建用户</Button>
            </form>}
            {users === null ? <p className="text-sm text-muted-foreground">正在加载用户…</p> : <div className="overflow-hidden rounded-lg border border-border bg-card">
              {users.map((user) => <div key={user.id} className="border-b border-border px-4 py-3 last:border-0">
                <div className="flex items-center justify-between gap-3">
                  <div className="min-w-0"><div className="font-medium">{user.display_name}</div><div className="truncate font-mono text-xs text-muted-foreground">{user.id}{user.email ? ` · ${user.email}` : ""}</div></div>
                  <div className="flex shrink-0 items-center gap-2">
                    <span className={`text-xs ${user.status === "active" ? "text-emerald-600" : "text-muted-foreground"}`}>{user.status === "active" ? "启用中" : "已停用"}</span>
                    {user.status === "active" ? <>
                      <Button size="sm" variant="outline" onClick={() => edit(user)} disabled={busy}><Pencil className="size-3.5" />编辑</Button>
                      <Button size="sm" variant="outline" className="text-destructive hover:text-destructive" onClick={() => void deactivate(user)} disabled={busy}><Trash2 className="size-3.5" />停用并清理</Button>
                    </> : <Button size="sm" onClick={() => void toggle(user)} disabled={busy}>启用</Button>}
                  </div>
                </div>
                {editingId === user.id && <div className="mt-3 grid gap-2 rounded-md bg-muted/40 p-3 sm:grid-cols-[1fr_1fr_auto]">
                  <Input value={profile.display_name} onChange={(event) => setProfile((current) => ({ ...current, display_name: event.target.value }))} placeholder="显示名称" />
                  <Input value={profile.email} onChange={(event) => setProfile((current) => ({ ...current, email: event.target.value }))} placeholder="邮箱（可留空）" type="email" />
                  <div className="flex gap-2"><Button size="sm" onClick={() => void saveProfile(user)} disabled={busy || !profile.display_name.trim()}><Save className="size-3.5" />保存</Button><Button size="sm" variant="ghost" onClick={() => setEditingId(null)} disabled={busy} aria-label="取消编辑"><X className="size-3.5" /></Button></div>
                </div>}
                {user.status === "active" && <div className="mt-2 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                  <span>停用前如需转移其智能体：</span>
                  <select value={transferTargets[user.id] ?? ""} onChange={(event) => setTransferTargets((current) => ({ ...current, [user.id]: event.target.value }))} className="h-8 max-w-[240px] rounded-md border border-input bg-transparent px-2 text-xs">
                    <option value="">未选择接收用户</option>
                    {users.filter((candidate) => candidate.id !== user.id && candidate.status === "active").map((candidate) => <option key={candidate.id} value={candidate.id}>{candidate.display_name}（{candidate.id}）</option>)}
                  </select>
                </div>}
              </div>)}</div>}
          </div>
        </main>
      </div>
    </div>
  );
}
