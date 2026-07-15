"use client";

import { Clock } from "lucide-react";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { DEFAULT_TIMEZONE, scheduleLabel } from "@/lib/schedule";

type ScheduleMode = "manual" | "hourly" | "daily" | "weekdays" | "weekly" | "monthly" | "custom";

interface ParsedSchedule {
  mode: ScheduleMode;
  time: string;
  dayOfWeek: string;
  dayOfMonth: string;
  customCron: string;
}

interface ScheduleEditorProps {
  cron: string;
  timezone: string;
  onChange: (next: { cron: string; timezone: string }) => void;
}

const DAYS = [
  ["1", "星期一"],
  ["2", "星期二"],
  ["3", "星期三"],
  ["4", "星期四"],
  ["5", "星期五"],
  ["6", "星期六"],
  ["0", "星期日"],
] as const;

const FREQUENCIES: Array<{ mode: ScheduleMode; label: string }> = [
  { mode: "manual", label: "按需运行" },
  { mode: "hourly", label: "每小时" },
  { mode: "daily", label: "每天" },
  { mode: "weekdays", label: "工作日" },
  { mode: "weekly", label: "每周" },
  { mode: "monthly", label: "每月" },
  { mode: "custom", label: "自定义" },
];

function toTime(hour: string, minute: string): string {
  return `${hour.padStart(2, "0")}:${minute.padStart(2, "0")}`;
}

function splitTime(time: string): [string, string] {
  const [hour = "09", minute = "00"] = time.split(":");
  return [String(Number(hour)), String(Number(minute))];
}

function clampInt(value: string, min: number, max: number): string {
  const n = Number.parseInt(value, 10);
  if (!Number.isFinite(n)) return String(min);
  return String(Math.min(max, Math.max(min, n)));
}

function parseCron(cron: string): ParsedSchedule {
  const raw = cron.trim().replace(/\s+/g, " ");
  const fallback: ParsedSchedule = {
    mode: raw ? "custom" : "manual",
    time: "09:00",
    dayOfWeek: "1",
    dayOfMonth: "1",
    customCron: raw,
  };

  if (!raw) return fallback;
  const parts = raw.split(" ");
  if (parts.length !== 5) return fallback;
  const [minute, hour, dayOfMonth, month, dayOfWeek] = parts;

  if (minute === "0" && hour === "*" && dayOfMonth === "*" && month === "*" && dayOfWeek === "*") {
    return { ...fallback, mode: "hourly" };
  }
  if (/^\d+$/.test(minute) && /^\d+$/.test(hour) && dayOfMonth === "*" && month === "*" && dayOfWeek === "*") {
    return { ...fallback, mode: "daily", time: toTime(hour, minute) };
  }
  if (/^\d+$/.test(minute) && /^\d+$/.test(hour) && dayOfMonth === "*" && month === "*" && dayOfWeek === "1-5") {
    return { ...fallback, mode: "weekdays", time: toTime(hour, minute) };
  }
  if (/^\d+$/.test(minute) && /^\d+$/.test(hour) && dayOfMonth === "*" && month === "*" && /^[0-6]$/.test(dayOfWeek)) {
    return { ...fallback, mode: "weekly", time: toTime(hour, minute), dayOfWeek };
  }
  if (/^\d+$/.test(minute) && /^\d+$/.test(hour) && /^\d+$/.test(dayOfMonth) && month === "*" && dayOfWeek === "*") {
    return { ...fallback, mode: "monthly", time: toTime(hour, minute), dayOfMonth };
  }

  return fallback;
}

function cronFor(next: ParsedSchedule): string {
  const [hour, minute] = splitTime(next.time);
  if (next.mode === "manual") return "";
  if (next.mode === "hourly") return "0 * * * *";
  if (next.mode === "daily") return `${minute} ${hour} * * *`;
  if (next.mode === "weekdays") return `${minute} ${hour} * * 1-5`;
  if (next.mode === "weekly") return `${minute} ${hour} * * ${next.dayOfWeek}`;
  if (next.mode === "monthly") return `${minute} ${hour} ${clampInt(next.dayOfMonth, 1, 31)} * *`;
  return next.customCron.trim();
}

export function ScheduleEditor({ cron, timezone, onChange }: ScheduleEditorProps) {
  const parsed = parseCron(cron);
  const tz = timezone || DEFAULT_TIMEZONE;

  const commit = (patch: Partial<ParsedSchedule>, nextTimezone = tz) => {
    const next = { ...parsed, ...patch };
    onChange({ cron: cronFor(next), timezone: nextTimezone || "UTC" });
  };

  return (
    <section className="grid gap-3 rounded-md border border-border bg-muted/10 p-3">
      <div className="flex items-center gap-2">
        <Clock className="size-3.5 text-muted-foreground" />
        <Label className="text-sm font-medium">运行计划</Label>
        <span className="min-w-0 truncate text-xs text-muted-foreground">
          {scheduleLabel(cron, tz)}
        </span>
      </div>

      <div className="grid gap-2 sm:grid-cols-[160px_1fr]">
        <div className="grid gap-1.5">
          <Label className="text-xs" htmlFor="schedule-frequency">频率</Label>
          <Select
            value={parsed.mode}
            onValueChange={(value) => value && commit({ mode: value as ScheduleMode })}
          >
            <SelectTrigger id="schedule-frequency" className="h-8 w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {FREQUENCIES.map((frequency) => (
                <SelectItem key={frequency.mode} value={frequency.mode}>
                  {frequency.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        {parsed.mode === "manual" && (
          <div className="flex items-end text-xs text-muted-foreground">
            仅在人工启动时运行。
          </div>
        )}

        {parsed.mode === "hourly" && (
          <div className="grid gap-1.5">
            <Label className="text-xs" htmlFor="schedule-hourly-tz">时区</Label>
            <Input
              id="schedule-hourly-tz"
              value={tz}
              onChange={(e) => onChange({ cron, timezone: e.target.value })}
              className="h-8 font-mono text-xs"
            />
          </div>
        )}

        {["daily", "weekdays", "weekly", "monthly"].includes(parsed.mode) && (
          <div className="grid gap-2 sm:grid-cols-3">
          {parsed.mode === "weekly" && (
            <div className="grid gap-1.5">
              <Label className="text-xs">星期</Label>
              <Select value={parsed.dayOfWeek} onValueChange={(value) => value && commit({ dayOfWeek: value })}>
                <SelectTrigger className="h-8 w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {DAYS.map(([value, label]) => (
                    <SelectItem key={value} value={value}>{label}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          )}
          {parsed.mode === "monthly" && (
            <div className="grid gap-1.5">
              <Label className="text-xs" htmlFor="schedule-month-day">日期</Label>
              <Input
                id="schedule-month-day"
                type="number"
                min={1}
                max={31}
                value={parsed.dayOfMonth}
                onChange={(e) => commit({ dayOfMonth: e.target.value })}
                className="h-8"
              />
            </div>
          )}
          <div className="grid gap-1.5">
            <Label className="text-xs" htmlFor="schedule-time">时间</Label>
            <Input
              id="schedule-time"
              type="time"
              value={parsed.time}
              onChange={(e) => commit({ time: e.target.value || "09:00" })}
              className="h-8"
            />
          </div>
          <div className="grid gap-1.5">
            <Label className="text-xs" htmlFor="schedule-tz">时区</Label>
            <Input
              id="schedule-tz"
              value={tz}
              onChange={(e) => onChange({ cron, timezone: e.target.value })}
              className="h-8 font-mono text-xs"
            />
          </div>
          </div>
        )}

        {parsed.mode === "custom" && (
          <div className="grid gap-2 sm:grid-cols-[1fr_180px]">
          <div className="grid gap-1.5">
            <Label className="text-xs" htmlFor="schedule-cron">CRON</Label>
            <Input
              id="schedule-cron"
              value={parsed.customCron}
              onChange={(e) => commit({ customCron: e.target.value })}
              placeholder="0 9 * * 1-5"
              className="h-8 font-mono text-xs"
            />
          </div>
          <div className="grid gap-1.5">
            <Label className="text-xs" htmlFor="schedule-custom-tz">时区</Label>
            <Input
              id="schedule-custom-tz"
              value={tz}
              onChange={(e) => onChange({ cron, timezone: e.target.value })}
              className="h-8 font-mono text-xs"
            />
          </div>
          </div>
        )}
      </div>
    </section>
  );
}
