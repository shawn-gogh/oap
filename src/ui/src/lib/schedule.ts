export const DEFAULT_TIMEZONE =
  Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";

export function scheduleLabel(cron?: string | null, timezone?: string | null): string {
  const expr = cron?.trim();
  if (!expr) return "按需运行";

  const normalized = expr.replace(/\s+/g, " ");
  const parts = normalized.split(" ");
  const tz = timezone?.trim() || "UTC";

  if (parts.length === 5) {
    const [minute, hour, dayOfMonth, month, dayOfWeek] = parts;
    if (minute === "0" && hour === "9" && dayOfMonth === "*" && month === "*" && dayOfWeek === "1-5") {
      return `工作日 09:00（${tz}）`;
    }
    if (minute === "0" && hour === "9" && dayOfMonth === "*" && month === "*" && dayOfWeek === "*") {
      return `每天 09:00（${tz}）`;
    }
    if (minute.startsWith("*/") && hour === "*" && dayOfMonth === "*" && month === "*" && dayOfWeek === "*") {
      return `每 ${minute.slice(2)} 分钟（${tz}）`;
    }
    if (minute === "0" && hour === "*" && dayOfMonth === "*" && month === "*" && dayOfWeek === "*") {
      return `每小时（${tz}）`;
    }
    if (minute === "0" && hour.startsWith("*/") && dayOfMonth === "*" && month === "*" && dayOfWeek === "*") {
      return `每 ${hour.slice(2)} 小时（${tz}）`;
    }
  }

  return `${normalized}（${tz}）`;
}
