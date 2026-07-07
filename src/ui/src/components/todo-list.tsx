import { Check, Circle, CircleDot } from "lucide-react";

export interface TodoItem {
  content: string;
  status: string;
}

const DONE_STATUSES = new Set(["completed", "done"]);
const ACTIVE_STATUSES = new Set(["in_progress", "in-progress", "active", "running"]);

function extractArray(value: unknown): unknown[] | null {
  // Some tool implementations put a JSON-encoded string in the output field
  // rather than the parsed structure, so try decoding it before giving up.
  if (typeof value === "string") {
    try {
      return extractArray(JSON.parse(value));
    } catch {
      return null;
    }
  }
  if (Array.isArray(value)) return value;
  if (value && typeof value === "object") {
    const obj = value as Record<string, unknown>;
    if (Array.isArray(obj.todos)) return obj.todos;
    if (Array.isArray(obj.items)) return obj.items;
  }
  return null;
}

/** Parses a todo-tool's input/output into a normalized item list, or null if the shape doesn't match. */
export function parseTodoItems(...candidates: unknown[]): TodoItem[] | null {
  for (const candidate of candidates) {
    const arr = extractArray(candidate);
    if (!arr || arr.length === 0) continue;
    const items: TodoItem[] = [];
    let ok = true;
    for (const raw of arr) {
      if (!raw || typeof raw !== "object") {
        ok = false;
        break;
      }
      const o = raw as Record<string, unknown>;
      const content = o.content ?? o.title ?? o.text ?? o.task;
      const status = o.status ?? o.state;
      if (typeof content !== "string" || typeof status !== "string") {
        ok = false;
        break;
      }
      items.push({ content, status });
    }
    if (ok) return items;
  }
  return null;
}

export function todoProgress(items: TodoItem[]): { done: number; total: number } {
  return { done: items.filter((i) => DONE_STATUSES.has(i.status.toLowerCase())).length, total: items.length };
}

function StatusIcon({ status }: { status: string }) {
  const s = status.toLowerCase();
  if (DONE_STATUSES.has(s)) return <Check className="size-3.5 text-emerald-600 dark:text-emerald-400" />;
  if (ACTIVE_STATUSES.has(s)) {
    return <CircleDot className="size-3.5 animate-pulse text-amber-600 motion-reduce:animate-none dark:text-amber-400" />;
  }
  return <Circle className="size-3.5 text-muted-foreground/60" />;
}

export function TodoList({ items }: { items: TodoItem[] }) {
  const { done, total } = todoProgress(items);
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
          <div
            className="h-full rounded-full bg-emerald-500 transition-all"
            style={{ width: total > 0 ? `${(done / total) * 100}%` : "0%" }}
          />
        </div>
        <span className="mono shrink-0 text-[10.5px] text-muted-foreground">
          {done}/{total}
        </span>
      </div>
      <ul className="flex flex-col gap-1.5">
        {items.map((item, i) => {
          const isDone = DONE_STATUSES.has(item.status.toLowerCase());
          return (
            <li key={i} className="flex items-start gap-2 text-[13px] leading-relaxed">
              <span className="mt-0.5 shrink-0">
                <StatusIcon status={item.status} />
              </span>
              <span className={isDone ? "text-muted-foreground line-through" : "text-foreground"}>{item.content}</span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
