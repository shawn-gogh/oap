"use client";

import { FormEvent, useEffect, useState } from "react";
import { Plus, Users } from "lucide-react";

import { Sidebar } from "@/components/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  createUser,
  listUsers,
  updateUserStatus,
  type ManagedUser,
} from "@/lib/api";

export default function UsersPage() {
  const [users, setUsers] = useState<ManagedUser[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [id, setId] = useState("");
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [busy, setBusy] = useState(false);

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
      await updateUserStatus(user.id, user.status === "active" ? "disabled" : "active");
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
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
              {users.map((user) => <div key={user.id} className="flex items-center justify-between gap-3 border-b border-border px-4 py-3 last:border-0">
                <div className="min-w-0"><div className="font-medium">{user.display_name}</div><div className="truncate font-mono text-xs text-muted-foreground">{user.id}{user.email ? ` · ${user.email}` : ""}</div></div>
                <Button size="sm" variant={user.status === "active" ? "outline" : "default"} onClick={() => void toggle(user)}>{user.status === "active" ? "停用" : "启用"}</Button>
              </div>)}</div>}
          </div>
        </main>
      </div>
    </div>
  );
}
