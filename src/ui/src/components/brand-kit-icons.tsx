"use client";

import React from "react";

/**
 * Brandkit Identity Icon System
 * Designed under strict `brandkit` visual guidelines:
 * - Construction Geometry & Precision Grids
 * - Negative Space Metaphors
 * - Product Action Alignment
 * - NO generic AI purple gradients (Strict Amber / Electric Blue / Emerald / Teal Palette)
 */

export interface BrandIconProps extends React.SVGProps<SVGSVGElement> {
  size?: number;
  className?: string;
}

/**
 * 1. Vault Brand Icon (凭证保险库)
 * Concept: Enclave Shield + Negative Space Key Core
 * Color Accent: Emerald & Steel (No purple)
 */
export function VaultBrandIcon({ size = 20, className = "", ...props }: BrandIconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
      {...props}
    >
      <path
        d="M12 2L3 7V12C3 17.52 7.03 21.74 12 23C16.97 21.74 21 17.52 21 12V7L12 2Z"
        stroke="currentColor"
        strokeWidth="1.75"
        strokeLinecap="round"
        strokeLinejoin="round"
        className="opacity-90"
      />
      <rect
        x="8.5"
        y="9.5"
        width="7"
        height="6"
        rx="1.5"
        stroke="currentColor"
        strokeWidth="1.5"
        className="text-emerald-500"
      />
      <circle cx="12" cy="12" r="1" fill="currentColor" className="text-emerald-500" />
      <path d="M12 13V14.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  );
}

/**
 * 2. Integrations Brand Icon (扩展集成 / MCP Hub)
 * Concept: Protocol Node Nexus + Multi-bus Interconnect
 * Color Accent: Electric Blue & Cyan (No purple)
 */
export function IntegrationsBrandIcon({ size = 20, className = "", ...props }: BrandIconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
      {...props}
    >
      <rect
        x="3"
        y="3"
        width="7"
        height="7"
        rx="2"
        stroke="currentColor"
        strokeWidth="1.75"
        className="text-blue-500"
      />
      <rect
        x="14"
        y="3"
        width="7"
        height="7"
        rx="2"
        stroke="currentColor"
        strokeWidth="1.75"
      />
      <rect
        x="14"
        y="14"
        width="7"
        height="7"
        rx="2"
        stroke="currentColor"
        strokeWidth="1.75"
        className="text-blue-500"
      />
      <rect
        x="3"
        y="14"
        width="7"
        height="7"
        rx="2"
        stroke="currentColor"
        strokeWidth="1.75"
      />
      <path
        d="M10 6.5H14M17.5 10V14M14 17.5H10M6.5 14V10"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeDasharray="2 2"
        className="opacity-60"
      />
      <circle cx="12" cy="12" r="1.5" fill="currentColor" className="text-blue-500" />
    </svg>
  );
}

/**
 * 3. Rules Brand Icon (系统规则 / System Prompt Laws)
 * Concept: Alignment Slits + Precision Metric Caliper
 * Color Accent: Warm Amber & Gold (Replaced purple to follow "avoid purple" rule!)
 */
export function RulesBrandIcon({ size = 20, className = "", ...props }: BrandIconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
      {...props}
    >
      <path
        d="M4 4C4 3.44772 4.44772 3 5 3H19C19.5523 3 20 3.44772 20 4V20C20 20.5523 19.5523 21 19 21H5C4.44772 21 4 20.5523 4 20V4Z"
        stroke="currentColor"
        strokeWidth="1.75"
        strokeLinecap="round"
      />
      <path
        d="M8 8H16M8 12H16M8 16H12"
        stroke="currentColor"
        strokeWidth="1.75"
        strokeLinecap="round"
        className="text-amber-500"
      />
      <path
        d="M15 16L16.5 17.5L19 14.5"
        stroke="currentColor"
        strokeWidth="1.75"
        strokeLinecap="round"
        strokeLinejoin="round"
        className="text-amber-600 dark:text-amber-400"
      />
    </svg>
  );
}

/**
 * 4. Skills Brand Icon (技能知识库 / Capability Matrix)
 * Concept: Modular Neural Prism + Skill Diamond
 * Color Accent: Cyber Teal & Mint (No purple)
 */
export function SkillsBrandIcon({ size = 20, className = "", ...props }: BrandIconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
      {...props}
    >
      <path
        d="M12 2L20.5 7V17L12 22L3.5 17V7L12 2Z"
        stroke="currentColor"
        strokeWidth="1.75"
        strokeLinecap="round"
        strokeLinejoin="round"
        className="opacity-90"
      />
      <path
        d="M12 6L16.5 9.5V14.5L12 18L7.5 14.5V9.5L12 6Z"
        stroke="currentColor"
        strokeWidth="1.5"
        className="text-teal-500"
      />
      <circle cx="12" cy="12" r="1.75" fill="currentColor" className="text-teal-400" />
    </svg>
  );
}

/**
 * 5. AI Gateway Brand Icon (AI 网关基础设施)
 * Concept: Mesh Routing Gateway + High-Dimension Pulse
 * Color Accent: Cyan & Electric Blue (No purple)
 */
export function AiGatewayBrandIcon({ size = 20, className = "", ...props }: BrandIconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
      {...props}
    >
      <rect
        x="3"
        y="4"
        width="18"
        height="16"
        rx="3"
        stroke="currentColor"
        strokeWidth="1.75"
        className="opacity-90"
      />
      <path
        d="M7 9H17M7 15H13"
        stroke="currentColor"
        strokeWidth="1.75"
        strokeLinecap="round"
        className="text-cyan-500"
      />
      <circle cx="17" cy="15" r="1.5" fill="currentColor" className="text-blue-500 animate-pulse" />
      <path
        d="M12 4V20"
        stroke="currentColor"
        strokeWidth="1"
        strokeDasharray="2 2"
        className="opacity-30"
      />
    </svg>
  );
}

/**
 * 6. Access Control Brand Icon (零信任访问控制)
 * Concept: Identity Lock Enclave + Vault Shield
 * Color Accent: Emerald & Steel Gold
 */
export function AccessControlBrandIcon({ size = 20, className = "", ...props }: BrandIconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
      {...props}
    >
      <path
        d="M12 2L4 6V11C4 16.55 7.4 21.74 12 23C16.6 21.74 20 16.55 20 11V6L12 2Z"
        stroke="currentColor"
        strokeWidth="1.75"
        strokeLinecap="round"
        strokeLinejoin="round"
        className="opacity-90"
      />
      <path
        d="M9 12L11 14L15 10"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        className="text-emerald-500"
      />
    </svg>
  );
}
