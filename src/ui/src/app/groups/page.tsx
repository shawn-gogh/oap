"use client";

import { FormEvent, useEffect, useState } from "react";
import { Plus, Trash2, Users } from "lucide-react";

import { Sidebar } from "@/components/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  addGroupMember,
  createGroup,
  deleteGroupMember,
  listGroupMembers,
  listGroups,
  listUsers,
  updateGroupStatus,
  type GroupMember,
  type ManagedGroup,
  type ManagedUser,
} from "@/lib/api";

export default function GroupsPage() {
  const [groups, setGroups] = useState<ManagedGroup[]>([]);
  const [users, setUsers] = useState<ManagedUser[]>([]);
  const [members, setMembers] = useState<GroupMember[]>([]);
  const [selectedGroup, setSelectedGroup] = useState("");
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [memberId, setMemberId] = useState("");
  const [memberRole, setMemberRole] = useState("member");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = async () => {
    try {
      const [groupRows, userRows] = await Promise.all([listGroups(), listUsers()]);
      setGroups(groupRows);
      setUsers(userRows.filter((user) => user.status === "active"));
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const loadMembers = async (groupId: string) => {
    if (!groupId) {
      setMembers([]);
      return;
    }
    try {
      setMembers(await listGroupMembers(groupId));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  useEffect(() => { void load(); }, []);
  useEffect(() => { void loadMembers(selectedGroup); }, [selectedGroup]);

  const create = async (event: FormEvent) => {
    event.preventDefault();
    if (!name.trim() || busy) return;
    setBusy(true);
    try {
      const group = await createGroup({ name: name.trim(), description: description.trim() || undefined });
      setName(""); setDescription("");
      await load();
      setSelectedGroup(group.id);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const addMember = async () => {
    if (!selectedGroup || !memberId || busy) return;
    setBusy(true);
    try {
      await addGroupMember(selectedGroup, memberId, memberRole);
      setMemberId("");
      await loadMembers(selectedGroup);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const removeMember = async (userId: string) => {
    if (!selectedGroup) return;
    try {
      await deleteGroupMember(selectedGroup, userId);
      await loadMembers(selectedGroup);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const toggleGroup = async (group: ManagedGroup) => {
    try {
      await updateGroupStatus(group.id, group.status === "active" ? "disabled" : "active");
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  return (
    <div className="flex h-screen bg-background text-foreground">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-12 shrink-0 items-center justify-between border-b border-border px-4"><div className="flex items-center gap-2"><Users className="size-4 text-muted-foreground" /><h1 className="text-sm font-semibold">用户组</h1></div><ThemeToggle /></header>
        <main id="main-content" className="flex-1 overflow-y-auto"><div className="mx-auto grid max-w-5xl gap-5 px-4 py-6 lg:grid-cols-[1fr_1.1fr]">
          <section className="space-y-4">
            <div><h2 className="text-lg font-semibold">用户组</h2><p className="text-sm text-muted-foreground">用户组可为智能体提供可复用的授权。</p></div>
            {error && <p className="rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error.startsWith("HTTP 403") ? "需要管理员权限。" : error}</p>}
            <form onSubmit={create} className="grid gap-2 rounded-lg border border-border bg-card p-4"><Input value={name} onChange={(event) => setName(event.target.value)} placeholder="用户组名称" /><Input value={description} onChange={(event) => setDescription(event.target.value)} placeholder="描述（可选）" /><Button type="submit" disabled={!name.trim() || busy}><Plus className="size-4" />创建用户组</Button></form>
            <div className="overflow-hidden rounded-lg border border-border bg-card">
              {groups.map((group) => <div key={group.id} className={`flex items-center justify-between gap-3 border-b border-border px-4 py-3 last:border-0 ${selectedGroup === group.id ? "bg-muted/50" : ""}`}><button type="button" className="min-w-0 text-left" onClick={() => setSelectedGroup(group.id)}><div className="font-medium">{group.name}</div><div className="truncate font-mono text-xs text-muted-foreground">{group.id}</div></button><Button size="sm" variant={group.status === "active" ? "outline" : "default"} onClick={() => void toggleGroup(group)}>{group.status === "active" ? "停用" : "启用"}</Button></div>)}
            </div>
          </section>
          <section className="space-y-4 rounded-lg border border-border bg-card p-4">
            <div><h2 className="text-lg font-semibold">成员</h2><p className="text-sm text-muted-foreground">{selectedGroup ? "管理当前用户组的成员。" : "请选择一个用户组以管理成员。"}</p></div>
            {selectedGroup && <div className="grid gap-2 sm:grid-cols-[1fr_auto_auto]"><select value={memberId} onChange={(event) => setMemberId(event.target.value)} className="h-9 rounded-md border border-input bg-transparent px-3 text-sm"><option value="">选择用户</option>{users.map((user) => <option key={user.id} value={user.id}>{user.display_name} ({user.id})</option>)}</select><select value={memberRole} onChange={(event) => setMemberRole(event.target.value)} className="h-9 rounded-md border border-input bg-transparent px-3 text-sm"><option value="member">成员</option><option value="group_admin">组管理员</option></select><Button onClick={() => void addMember()} disabled={!memberId || busy}><Plus className="size-4" />添加</Button></div>}
            {members.length === 0 ? <p className="text-sm text-muted-foreground">暂无成员。</p> : <div className="divide-y divide-border">{members.map((member) => <div key={member.user_id} className="flex items-center justify-between py-2"><div><span className="font-mono text-sm">{member.user_id}</span><span className="ml-2 text-xs text-muted-foreground">{member.member_role === "group_admin" ? "组管理员" : "成员"}</span></div><Button variant="ghost" size="icon-sm" className="text-destructive" onClick={() => void removeMember(member.user_id)} aria-label={`移除 ${member.user_id}`}><Trash2 className="size-4" /></Button></div>)}</div>}
          </section>
        </div></main>
      </div>
    </div>
  );
}
