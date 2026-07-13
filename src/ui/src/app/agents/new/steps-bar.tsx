"use client";

import { cn } from "@/lib/utils";
import type { BuilderStep } from "./builder-shared";

const BUILDER_STEPS: Array<{ index: 1 | 2 | 3 | 4; step: BuilderStep; label: string; suffix?: string }> = [
  { index: 1, step: "create", label: "定位 Fit" },
  { index: 2, step: "eval", label: "评估 Eval" },
  { index: 3, step: "config", label: "设计 Design" },
  { index: 4, step: "review", label: "复核 Review", suffix: "POST /api/agents" },
];

export function PlatformSteps({
  activeStep,
  canEnterConfig,
  canEnterReview,
  onNavigate,
}: {
  activeStep: 1 | 2 | 3 | 4;
  canEnterConfig: boolean;
  canEnterReview: boolean;
  onNavigate: (step: BuilderStep) => void;
}) {
  const stepEnabled = (index: 1 | 2 | 3 | 4): boolean => {
    // Backward navigation is always allowed; forward jumps must pass the
    // same gates as the in-page buttons (eval gate, then a valid config).
    if (index <= activeStep) return true;
    if (index === 2) return activeStep >= 1;
    if (index === 3) return canEnterConfig;
    return canEnterConfig && canEnterReview;
  };
  return (
    <div className="border-b border-border bg-background/80 px-4 py-3 backdrop-blur">
      <div className="mx-auto flex max-w-7xl items-center gap-3">
        {BUILDER_STEPS.map((entry, position) => (
          <div key={entry.step} className="flex min-w-0 items-center gap-3">
            {position > 0 && <div className="h-px w-10 bg-border" />}
            <StepMarker
              active={activeStep === entry.index}
              clickable={stepEnabled(entry.index)}
              index={entry.index}
              label={entry.label}
              suffix={entry.suffix}
              onClick={() => {
                if (entry.index !== activeStep && stepEnabled(entry.index)) onNavigate(entry.step);
              }}
            />
          </div>
        ))}
      </div>
    </div>
  );
}

function StepMarker({
  active,
  clickable,
  index,
  label,
  suffix,
  onClick,
}: {
  active: boolean;
  clickable: boolean;
  index: number;
  label: string;
  suffix?: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={!clickable}
      className={cn(
        "flex min-w-0 items-center gap-2 rounded-md px-1 py-0.5",
        active ? "text-foreground" : "text-muted-foreground",
        clickable && !active && "cursor-pointer hover:text-foreground",
        !clickable && "cursor-default opacity-60",
      )}
    >
      <span
        className={cn(
          "flex size-6 shrink-0 items-center justify-center rounded-full text-xs font-semibold",
          active ? "bg-foreground text-background" : "bg-muted text-muted-foreground",
        )}
      >
        {index}
      </span>
      <span className="truncate text-sm font-semibold">{label}</span>
      {suffix && <span className="hidden font-mono text-xs text-muted-foreground sm:inline">{suffix}</span>}
    </button>
  );
}
