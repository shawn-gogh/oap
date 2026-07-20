"use client";

import { FormEvent, useEffect, useState } from "react";
import { Plus, Trash2, Users, ShieldCheck, Lock } from "lucide-react";
import { AccessControlBrandIcon } from "@/components/brand-kit-icons";
import { Sidebar } from "@/components/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  addGroupMember,
  createGroup,
  deleteGroupAgentGrant,
  deleteGroupMember,
  listGroupAgentGrants,
  listGroupMembers,
  listGroups,
  listUsers,
  updateGroupStatus,
  type GroupMember,
  type AgentGroupGrant,
  type ManagedGroup,
  type ManagedUser,
} from "@/lib/api";

export default function GroupsPage() {
  const [groups, setGroups] = useState<ManagedGroup[]>([]);
  const [users, setUsers] = useState<ManagedUser[]>([]);
  const [members, setMembers] = useState<GroupMember[]>([]);
  const [agentGrants, setAgentGrants] = useState<AgentGroupGrant[]>([]);
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
      setAgentGrants([]);
      return;
    }
    try {
      const [memberRows, grantRows] = await Promise.all([listGroupMembers(groupId), listGroupAgentGrants(groupId)]);
      setMembers(memberRows);
      setAgentGrants(grantRows);
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

  const removeAgentGrant = async (agentId: string) => {
    if (!selectedGroup) return;
    try {
      await deleteGroupAgentGrant(selectedGroup, agentId);
      await loadMembers(selectedGroup);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

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
              <span className="text-xs text-muted-foreground font-medium">/ 用户组与授权</span>
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
                      <Lock className="size-3" /> 用户组策略
                    </span>
                    <span className="text-xs text-muted-foreground font-medium">授权与策略映射</span>
                  </div>
                  <h1 className="text-xl font-bold tracking-tight text-foreground">
                    用户组及批量智能体授权
                  </h1>
                  <p className="text-xs text-muted-foreground leading-relaxed">
                    创建用户组、分配组员与组管理员。可对整个用户组批量授予指定智能体的调起与操作权限。
                  </p>
                </div>
              </div>
            </div>

            {error && (
              <div className="rounded-xl border border-destructive/40 bg-destructive/10 p-3.5 text-xs font-mono text-destructive">
                {error.startsWith("HTTP 403") ? "需要管理员权限。" : error}
              </div>
            )}

            <div className="grid gap-6 lg:grid-cols-[1fr_1.1fr]">
              <section className="space-y-4">
                <form onSubmit={create} className="grid gap-3 rounded-2xl border border-border/70 bg-card p-4 shadow-2xs">
                  <Input value={name} onChange={(event) => setName(event.target.value)} placeholder="用户组名称" className="h-9 text-xs" />
                  <Input value={description} onChange={(event) => setDescription(event.target.value)} placeholder="描述（可选）" className="h-9 text-xs" />
                  <Button type="submit" size="sm" disabled={!name.trim() || busy} className="h-9 text-xs bg-emerald-600 hover:bg-emerald-700 text-white font-medium gap-1">
                    <Plus className="size-3.5" />
                    创建用户组
                  </Button>
                </form>
                <div className="overflow-hidden rounded-2xl border border-border/70 bg-card shadow-2xs">
                  {groups.map((group) => (
                    <div key={group.id} className={`flex items-center justify-between gap-3 border-b border-border/60 px-4 py-3 last:border-0 hover:bg-muted/30 transition-colors ${selectedGroup === group.id ? "bg-muted/50 border-l-2 border-l-emerald-500" : ""}`}>
                      <button type="button" className="min-w-0 text-left" onClick={() => setSelectedGroup(group.id)}>
                        <div className="font-semibold text-xs text-foreground">{group.name}</div>
                        <div className="truncate font-mono text-[11px] text-muted-foreground mt-0.5">{group.id}</div>
                      </button>
                      <Button size="sm" variant={group.status === "active" ? "outline" : "default"} className="h-7 text-xs" onClick={() => void toggleGroup(group)}>
                        {group.status === "active" ? "停用" : "启用"}
                      </Button>
                    </div>
                  ))}
                </div>
              </section>

              <section className="space-y-4 rounded-2xl border border-border/70 bg-card p-5 shadow-2xs">
                <div>
                  <h2 className="text-sm font-bold tracking-tight text-foreground">组成员管理</h2>
                  <p className="text-xs text-muted-foreground mt-0.5">{selectedGroup ? "管理当前选中用户组的成员与权限角色。" : "请在左侧选择一个用户组查看详情。"}</p>
                </div>
                {selectedGroup && (
                  <div className="grid gap-2 sm:grid-cols-[1fr_auto_auto] items-center">
                    <select value={memberId} onChange={(event) => setMemberId(event.target.value)} className="h-9 rounded-xl border border-border bg-background px-3 text-xs text-foreground font-mono">
                      <option value="">选择用户</option>
                      {users.map((user) => (
                        <option key={user.id} value={user.id}>{user.display_name} ({user.id})</option>
                      ))}
                    </select>
                    <select value={memberRole} onChange={(event) => setMemberRole(event.target.value)} className="h-9 rounded-xl border border-border bg-background px-3 text-xs text-foreground">
                      <option value="member">普通成员</option>
                      <option value="group_admin">组管理员</option>
                    </select>
                    <Button size="sm" className="h-9 text-xs bg-emerald-600 hover:bg-emerald-700 text-white font-medium" onClick={() => void addMember()} disabled={!memberId || busy}>
                      <Plus className="size-3.5" />
                      添加
                    </Button>
                  </div>
                )}
                {members.length === 0 ? (
                  <p className="text-xs text-muted-foreground font-mono">暂无成员。</p>
                ) : (
                  <div className="divide-y divide-border/60">
                    {members.map((member) => (
                      <div key={member.user_id} className="flex items-center justify-between py-2 text-xs">
                        <div>
                          <span className="font-mono font-medium text-foreground">{member.user_id}</span>
                          <span className="ml-2 text-[10px] px-1.5 py-0.5 rounded bg-muted text-muted-foreground">{member.member_role === "group_admin" ? "组管理员" : "普通成员"}</span>
                        </div>
                        <Button variant="ghost" size="icon-sm" className="text-destructive hover:bg-destructive/10" onClick={() => void removeMember(member.user_id)} aria-label={`移除 ${member.user_id}`}>
                          <Trash2 className="size-3.5" />
                        </Button>
                      </div>
                    ))}
                  </div>
                )}

                <div className="border-t border-border/70 pt-4">
                  <h2 className="text-sm font-bold tracking-tight text-foreground">已授权智能体列表</h2>
                  <p className="text-xs text-muted-foreground mt-0.5">组管理员可以管理或撤销本组关联的智能体使用授权。</p>
                </div>
                {agentGrants.length === 0 ? (
                  <p className="text-xs text-muted-foreground font-mono">暂无智能体授权。</p>
                ) : (
                  <div className="divide-y divide-border/60">
                    {agentGrants.map((grant) => (
                      <div key={grant.id} className="flex items-center justify-between py-2 text-xs">
                        <div>
                          <span className="font-mono font-medium text-foreground">{grant.agent_id}</span>
                          <span className="ml-2 text-[10px] px-1.5 py-0.5 rounded border border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400">{grant.permission}</span>
                        </div>
                        <Button variant="ghost" size="icon-sm" className="text-destructive hover:bg-destructive/10" onClick={() => void removeAgentGrant(grant.agent_id)} aria-label={`撤销 ${grant.agent_id} 的授权`}>
                          <Trash2 className="size-3.5" />
                        </Button>
                      </div>
                    ))}
                  </div>
                )}
              </section>
            </div>
          </div>
        </main>
      </div>
    </div>
  );
}
