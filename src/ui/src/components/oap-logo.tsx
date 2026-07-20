"use client";

import React from "react";

export interface OapLogoProps {
  /** Size of the mark icon in px */
  size?: number;
  /** Whether to show the text label "OAP 开放智能体平台" or "OAP" next to mark */
  showText?: boolean;
  /** Custom text subtitle or badge */
  subtitle?: string;
  className?: string;
  textClassName?: string;
}

/**
 * Brandkit Identity Logo for OAP (Open Agent Platform)
 * 
 * Design Concept:
 * - Metaphor: Autonomous Agent Orbit + Neural Core + Open Gateway
 * - Geometry: Outer precision enclave ring with negative-space 45° gateway cut.
 * - Center: High-density glowing Agent Intelligence Spark.
 * - Palette: Emerald to Electric Blue gradient (No purple slop).
 */
export function OapLogoMark({ size = 24, className = "" }: { size?: number; className?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={`shrink-0 ${className}`}
    >
      <defs>
        <linearGradient id="oap-logo-grad" x1="2" y1="2" x2="30" y2="30" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#10b981" />
          <stop offset="50%" stopColor="#06b6d4" />
          <stop offset="100%" stopColor="#3b82f6" />
        </linearGradient>
        <linearGradient id="oap-core-grad" x1="10" y1="10" x2="22" y2="22" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#34d399" />
          <stop offset="100%" stopColor="#60a5fa" />
        </linearGradient>
      </defs>

      {/* Outer Enclave Ring with Open Gateway Cutout (Negative Space) */}
      <path
        d="M20 4.5C23.6 6 26.5 8.9 28 12.5M28 19.5C26.5 23.1 23.6 26 20 27.5M12 27.5C8.4 26 5.5 23.1 4 19.5M4 12.5C5.5 8.9 8.4 6 12 4.5"
        stroke="url(#oap-logo-grad)"
        strokeWidth="2.5"
        strokeLinecap="round"
      />

      {/* Gateway Connector Nodes */}
      <circle cx="20" cy="4.5" r="1.75" fill="#10b981" />
      <circle cx="28" cy="12.5" r="1.75" fill="#06b6d4" />
      <circle cx="12" cy="27.5" r="1.75" fill="#3b82f6" />
      <circle cx="4" cy="12.5" r="1.75" fill="#10b981" />

      {/* Center Intelligence Spark (Agent Core) */}
      <path
        d="M16 8.5C16 12.5 12.5 16 8.5 16C12.5 16 16 19.5 16 23.5C16 19.5 19.5 16 23.5 16C19.5 16 16 12.5 16 8.5Z"
        fill="url(#oap-core-grad)"
      />

      {/* Orbit Intersect Core Ring */}
      <circle
        cx="16"
        cy="16"
        r="3.5"
        stroke="currentColor"
        strokeWidth="1.25"
        className="text-background"
      />
    </svg>
  );
}

export function OapLogo({
  size = 24,
  showText = true,
  subtitle,
  className = "",
  textClassName = "",
}: OapLogoProps) {
  return (
    <div className={`flex items-center gap-2.5 ${className}`}>
      <div className="relative flex items-center justify-center">
        <OapLogoMark size={size} />
      </div>

      {showText && (
        <div className={`flex flex-col leading-none ${textClassName}`}>
          <div className="flex items-center gap-1.5">
            <span className="font-mono font-bold tracking-tight text-foreground text-sm sm:text-base">
              OAP
            </span>
            <span className="text-xs font-semibold text-foreground/90 tracking-tight">
              开放智能体平台
            </span>
          </div>
          {subtitle && (
            <span className="mt-0.5 text-[10px] font-mono text-muted-foreground tracking-wider uppercase">
              {subtitle}
            </span>
          )}
        </div>
      )}
    </div>
  );
}
