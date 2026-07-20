// Brand logos for integrations, rendered as inline SVG so they scale crisply
// and pick up no external assets. Keyed by integration id.

import type { ReactNode, SVGProps } from "react";

function AnthropicIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg" {...props}>
      <path
        fill="currentColor"
        d="M32.2 10h-5.8l10.6 28h5.8L32.2 10ZM15.8 10 5.2 38h5.9l2.2-6.2h11.4l2.2 6.2h5.9L22.2 10h-6.4Zm-.8 16.9L19 15.6l4 11.3h-8Z"
      />
    </svg>
  );
}

function GmailIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg" {...props}>
      <path fill="#4caf50" d="M45 16.2l-5 2.75-5 4.75L35 40h7c1.657 0 3-1.343 3-3V16.2z" />
      <path fill="#1e88e5" d="M3 16.2l3.614 1.71L13 23.7V40H6c-1.657 0-3-1.343-3-3V16.2z" />
      <polygon fill="#e53935" points="35,11.2 24,19.45 13,11.2 12,17 13,23.7 24,31.95 35,23.7 36,17" />
      <path fill="#c62828" d="M3 12.298V16.2l10 7.5V11.2L9.876 8.859C9.132 8.301 8.228 8 7.298 8 4.924 8 3 9.924 3 12.298z" />
      <path fill="#fbc02d" d="M45 12.298V16.2l-10 7.5V11.2l3.124-2.341C38.868 8.301 39.772 8 40.702 8 43.076 8 45 9.924 45 12.298z" />
    </svg>
  );
}

function LinearIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" {...props}>
      <path
        fill="#5E6AD2"
        d="M2.886 4.18A11.982 11.982 0 0 1 11.838 0L2.886 8.952V4.18ZM.21 9.683 9.683.21a11.987 11.987 0 0 0-3.092 1.149L1.36 6.59A11.987 11.987 0 0 0 .21 9.683Zm.045 4.052L13.735.255a12.018 12.018 0 0 0-1.79.097L.352 11.945a12.018 12.018 0 0 0-.097 1.79Zm.836 3.456L17.243 1.09a12.06 12.06 0 0 0-1.371-.71L.38 15.872c.18.484.418.943.71 1.371Zm2.04 2.51L19.728 3.66a12.066 12.066 0 0 0-1.04-1.184L2.475 18.69c.367.385.763.732 1.184 1.04Zm3.158 1.86L21.96 6.752a11.918 11.918 0 0 0-.72-1.398L5.354 21.24c.443.275.911.516 1.398.72ZM12 24c6.627 0 12-5.373 12-12 0-.34-.014-.675-.041-1.008L11.008 23.96c.333.027.668.041 1.008.041Z"
      />
    </svg>
  );
}

function PylonIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg" {...props}>
      <rect width="32" height="32" rx="8" fill="#6D4AFF" />
      <path
        fill="none"
        stroke="#fff"
        strokeWidth="2"
        strokeLinecap="round"
        d="M16 7a9 9 0 1 0 9 9"
      />
      <circle cx="16" cy="16" r="3.2" fill="#fff" />
    </svg>
  );
}

function SlackIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 122.8 122.8" xmlns="http://www.w3.org/2000/svg" {...props}>
      <path
        fill="#36C5F0"
        d="M25.8 77.6c0 7.1-5.8 12.9-12.9 12.9S0 84.7 0 77.6s5.8-12.9 12.9-12.9h12.9v12.9zm6.5 0c0-7.1 5.8-12.9 12.9-12.9s12.9 5.8 12.9 12.9v32.3c0 7.1-5.8 12.9-12.9 12.9s-12.9-5.8-12.9-12.9V77.6z"
      />
      <path
        fill="#2EB67D"
        d="M45.2 25.8c-7.1 0-12.9-5.8-12.9-12.9S38.1 0 45.2 0s12.9 5.8 12.9 12.9v12.9H45.2zm0 6.5c7.1 0 12.9 5.8 12.9 12.9s-5.8 12.9-12.9 12.9H12.9C5.8 58.1 0 52.3 0 45.2s5.8-12.9 12.9-12.9h32.3z"
      />
      <path
        fill="#ECB22E"
        d="M97 45.2c0-7.1 5.8-12.9 12.9-12.9s12.9 5.8 12.9 12.9-5.8 12.9-12.9 12.9H97V45.2zm-6.5 0c0 7.1-5.8 12.9-12.9 12.9s-12.9-5.8-12.9-12.9V12.9C64.7 5.8 70.5 0 77.6 0s12.9 5.8 12.9 12.9v32.3z"
      />
      <path
        fill="#E01E5A"
        d="M77.6 97c7.1 0 12.9 5.8 12.9 12.9s-5.8 12.9-12.9 12.9-12.9-5.8-12.9-12.9V97h12.9zm0-6.5c-7.1 0-12.9-5.8-12.9-12.9s5.8-12.9 12.9-12.9h32.3c7.1 0 12.9 5.8 12.9 12.9s-5.8 12.9-12.9 12.9H77.6z"
      />
    </svg>
  );
}

function TeamsIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="4 4 36 38" fill="none" xmlns="http://www.w3.org/2000/svg" {...props}>
      <path
        fill="url(#teams-a)"
        d="M21.9999 20h12c3.3137 0 6 2.6863 6 6v10c0 3.3137-2.6863 6-6 6s-6-2.6863-6-6V26c0-3.3137-2.6863-6-6-6"
      />
      <path
        fill="url(#teams-b)"
        d="M7.99988 24c0-3.3137 2.68632-6 6.00002-6h8c3.3137 0 6 2.6863 6 6v12c0 3.3137 2.6863 6 6 6l-16.0001-.0001c-5.5228 0-9.99992-4.4771-9.99992-10z"
      />
      <path
        fill="url(#teams-c)"
        fillOpacity=".7"
        d="M7.99988 24c0-3.3137 2.68632-6 6.00002-6h8c3.3137 0 6 2.6863 6 6v12c0 3.3137 2.6863 6 6 6l-16.0001-.0001c-5.5228 0-9.99992-4.4771-9.99992-10z"
      />
      <path
        fill="url(#teams-d)"
        fillOpacity=".7"
        d="M7.99988 24c0-3.3137 2.68632-6 6.00002-6h8c3.3137 0 6 2.6863 6 6v12c0 3.3137 2.6863 6 6 6l-16.0001-.0001c-5.5228 0-9.99992-4.4771-9.99992-10z"
      />
      <path
        fill="url(#teams-e)"
        d="M32.9999 18c2.7614 0 5-2.2386 5-5s-2.2386-5-5-5-5 2.2386-5 5 2.2386 5 5 5"
      />
      <path
        fill="url(#teams-f)"
        fillOpacity=".46"
        d="M32.9999 18c2.7614 0 5-2.2386 5-5s-2.2386-5-5-5-5 2.2386-5 5 2.2386 5 5 5"
      />
      <path
        fill="url(#teams-g)"
        fillOpacity=".4"
        d="M32.9999 18c2.7614 0 5-2.2386 5-5s-2.2386-5-5-5-5 2.2386-5 5 2.2386 5 5 5"
      />
      <path
        fill="url(#teams-h)"
        d="M17.9999 16c3.3137 0 6-2.6863 6-6 0-3.31371-2.6863-6-6-6s-6 2.68629-6 6c0 3.3137 2.6863 6 6 6"
      />
      <path
        fill="url(#teams-i)"
        fillOpacity=".6"
        d="M17.9999 16c3.3137 0 6-2.6863 6-6 0-3.31371-2.6863-6-6-6s-6 2.68629-6 6c0 3.3137 2.6863 6 6 6"
      />
      <path
        fill="url(#teams-j)"
        fillOpacity=".5"
        d="M17.9999 16c3.3137 0 6-2.6863 6-6 0-3.31371-2.6863-6-6-6s-6 2.68629-6 6c0 3.3137 2.6863 6 6 6"
      />
      <rect width="16" height="16" x="4" y="23" fill="url(#teams-k)" rx="3.25" />
      <rect width="16" height="16" x="4" y="23" fill="url(#teams-l)" fillOpacity=".7" rx="3.25" />
      <path fill="#fff" d="M15.4792 28.1054h-2.4471v7.466h-2.0648v-7.466H8.52014v-1.6768h6.95906z" />
      <defs>
        <radialGradient
          id="teams-a"
          cx="0"
          cy="0"
          r="1"
          gradientTransform="matrix(13.4784 0 0 33.2694 39.7967 22.1739)"
          gradientUnits="userSpaceOnUse"
        >
          <stop stopColor="#a98aff" />
          <stop offset=".14" stopColor="#8c75ff" />
          <stop offset=".565" stopColor="#5f50e2" />
          <stop offset=".9" stopColor="#3c2cb8" />
        </radialGradient>
        <radialGradient
          id="teams-b"
          cx="0"
          cy="0"
          r="1"
          gradientTransform="rotate(68.1539 -7.71566095 14.71355834)scale(32.752 33.1231)"
          gradientUnits="userSpaceOnUse"
        >
          <stop stopColor="#85c2ff" />
          <stop offset=".69" stopColor="#7588ff" />
          <stop offset="1" stopColor="#6459fe" />
        </radialGradient>
        <linearGradient id="teams-c" x1="20.5936" x2="20.5936" y1="18" y2="42" gradientUnits="userSpaceOnUse">
          <stop offset=".801159" stopColor="#6864f6" stopOpacity="0" />
          <stop offset="1" stopColor="#5149de" />
        </linearGradient>
        <radialGradient
          id="teams-d"
          cx="0"
          cy="0"
          r="1"
          gradientTransform="rotate(113.326 8.09285255 17.64474501)scale(19.2186 15.4273)"
          gradientUnits="userSpaceOnUse"
        >
          <stop stopColor="#bd96ff" />
          <stop offset=".686685" stopColor="#bd96ff" stopOpacity="0" />
        </radialGradient>
        <radialGradient
          id="teams-e"
          cx="0"
          cy="0"
          r="1"
          gradientTransform="matrix(0 -10 12.6216 0 32.9999 11.5714)"
          gradientUnits="userSpaceOnUse"
        >
          <stop offset=".268201" stopColor="#6868f7" />
          <stop offset="1" stopColor="#3923b1" />
        </radialGradient>
        <radialGradient
          id="teams-f"
          cx="0"
          cy="0"
          r="1"
          gradientTransform="rotate(40.0516 -.03068196 44.8729095)scale(7.14629 10.3363)"
          gradientUnits="userSpaceOnUse"
        >
          <stop offset=".270711" stopColor="#a1d3ff" />
          <stop offset=".813393" stopColor="#a1d3ff" stopOpacity="0" />
        </radialGradient>
        <radialGradient
          id="teams-g"
          cx="0"
          cy="0"
          r="1"
          gradientTransform="rotate(-41.6581 32.11799918 -43.41948423)scale(8.51275 20.8824)"
          gradientUnits="userSpaceOnUse"
        >
          <stop stopColor="#e3acfd" />
          <stop offset=".816041" stopColor="#9fa2ff" stopOpacity="0" />
        </radialGradient>
        <radialGradient
          id="teams-h"
          cx="0"
          cy="0"
          r="1"
          gradientTransform="matrix(0 -12 15.146 0 17.9999 8.28571)"
          gradientUnits="userSpaceOnUse"
        >
          <stop offset=".268201" stopColor="#8282ff" />
          <stop offset="1" stopColor="#3923b1" />
        </radialGradient>
        <radialGradient
          id="teams-i"
          cx="0"
          cy="0"
          r="1"
          gradientTransform="rotate(40.0516 -3.15465147 21.41641466)scale(8.57554 12.4035)"
          gradientUnits="userSpaceOnUse"
        >
          <stop offset=".270711" stopColor="#a1d3ff" />
          <stop offset=".813393" stopColor="#a1d3ff" stopOpacity="0" />
        </radialGradient>
        <radialGradient
          id="teams-j"
          cx="0"
          cy="0"
          r="1"
          gradientTransform="rotate(-41.6581 20.38180375 -26.51566158)scale(10.2153 25.0589)"
          gradientUnits="userSpaceOnUse"
        >
          <stop stopColor="#e3acfd" />
          <stop offset=".816041" stopColor="#9fa2ff" stopOpacity="0" />
        </radialGradient>
        <radialGradient
          id="teams-k"
          cx="0"
          cy="0"
          r="1"
          gradientTransform="rotate(45 -25.76345597 16.32842712)scale(22.6274)"
          gradientUnits="userSpaceOnUse"
        >
          <stop offset=".046875" stopColor="#688eff" />
          <stop offset=".946875" stopColor="#230f94" />
        </radialGradient>
        <radialGradient
          id="teams-l"
          cx="0"
          cy="0"
          r="1"
          gradientTransform="matrix(0 11.2 -13.0702 0 12 32.6)"
          gradientUnits="userSpaceOnUse"
        >
          <stop offset=".570647" stopColor="#6965f6" stopOpacity="0" />
          <stop offset="1" stopColor="#8f8fff" />
        </radialGradient>
      </defs>
    </svg>
  );
}

function ClaudeIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 600 600" xmlns="http://www.w3.org/2000/svg" {...props}>
      <path
        fill="#D97757"
        fillRule="evenodd"
        clipRule="evenodd"
        d="M525 273.7h75v77.6h-75V427h-37.2v73H450v-73h-37.2v73H375v-73H225v73h-37.8v-73H150v73h-37.8v-73H75v-75.7H0v-77.6h75V125h450zm-375 0h37.2v-71.1H150zm262.8 0H450v-71.1h-37.2z"
      />
    </svg>
  );
}

function CodexIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg" {...props}>
      <defs>
        <linearGradient id="codex-cloud" x1="12" x2="36" y1="7" y2="43" gradientUnits="userSpaceOnUse">
          <stop stopColor="#D7DEFF" />
          <stop offset=".42" stopColor="#8B8CFF" />
          <stop offset="1" stopColor="#124BFF" />
        </linearGradient>
      </defs>
      <path
        fill="url(#codex-cloud)"
        d="M15.7 39.5c-6.3 0-11.2-4.3-11.2-10.3 0-5 3.4-9 8-10.1C14 12.2 20 7.5 27 8.6c5 .8 8.8 4 10.4 8.4 4.3.9 7.1 4.8 7.1 9.3 0 5.7-4.7 10.2-10.6 10.2H15.7Z"
      />
      <path
        fill="none"
        stroke="#fff"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="3.4"
        d="m18.6 21.2 4.2 4.2-4.2 4.2M28.4 29.6h6"
      />
    </svg>
  );
}

function BedrockAgentCoreIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg" {...props}>
      <defs>
        <linearGradient id="agentcore-bg" x1="4" x2="44" y1="44" y2="4" gradientUnits="userSpaceOnUse">
          <stop stopColor="#3B1D8F" />
          <stop offset=".48" stopColor="#7C3AED" />
          <stop offset="1" stopColor="#9B5CFF" />
        </linearGradient>
      </defs>
      <rect width="48" height="48" rx="9" fill="url(#agentcore-bg)" />
      <path
        fill="none"
        stroke="#fff"
        strokeLinejoin="round"
        strokeWidth="2.8"
        d="m17 10 8-4.5 8 4.5v10l-5 3 5 3v10l-8 4.5-8-4.5v-10l5-3-5-3z"
      />
      <path
        fill="none"
        stroke="#fff"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="2.8"
        d="M25 10v10l-4 2.5M25 40V30l4-2.5"
      />
      <path
        fill="none"
        stroke="#fff"
        strokeLinejoin="round"
        strokeWidth="2.8"
        d="m36.5 16.5 3.1 6.4 6.4 3.1-6.4 3.1-3.1 6.4-3.1-6.4-6.4-3.1 6.4-3.1z"
      />
    </svg>
  );
}

function CursorIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg" {...props}>
      <rect width="48" height="48" rx="10" fill="#000" />
      <path
        fill="#fff"
        d="M23.2 8.9a3.2 3.2 0 0 1 3.2 0l14.9 8.7a3.2 3.2 0 0 1 1.6 2.8v17.2a3.2 3.2 0 0 1-1.6 2.8l-14.9 8.7a3.2 3.2 0 0 1-3.2 0L8.7 40.4a3.2 3.2 0 0 1-1.6-2.8V20.4a3.2 3.2 0 0 1 1.6-2.8l14.5-8.7Z"
      />
      <path
        fill="#000"
        d="M12.2 19.4h23.4a.8.8 0 0 1 .7 1.2L25.1 40.1a.8.8 0 0 1-1.5-.1l-3.9-11.4a2 2 0 0 0-.8-1.1L11.8 21a.8.8 0 0 1 .4-1.5Z"
      />
    </svg>
  );
}

function GeminiIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg" {...props}>
      <rect width="48" height="48" rx="10" fill="#0B57D0" />
      <path
        fill="#fff"
        d="M24 7.5c1.7 7.9 6.6 12.8 16.5 16.5C30.6 27.7 25.7 32.6 24 40.5 22.3 32.6 17.4 27.7 7.5 24 17.4 20.3 22.3 15.4 24 7.5Z"
      />
      <path
        fill="#AECBFA"
        d="M33.8 6.8c.7 3.2 2.7 5.2 6.7 6.7-4 1.5-6 3.5-6.7 6.7-.7-3.2-2.7-5.2-6.7-6.7 4-1.5 6-3.5 6.7-6.7Z"
      />
    </svg>
  );
}

function ElasticIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 640 751" xmlns="http://www.w3.org/2000/svg" {...props}>
      <image
        width="640"
        height="751"
        preserveAspectRatio="xMidYMid meet"
        href="https://s.yimg.com/ny/api/res/1.2/mpd9_NXnN2ZqM1N7v88Mcw--/YXBwaWQ9aGlnaGxhbmRlcjt3PTY0MDtoPTc1MQ--/https://media.zenfs.com/en/business-wire.com/bc636fdaf19fd0c9bf2eaff6413be095"
      />
    </svg>
  );
}

function OpenCodeIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg" {...props}>
      <rect width="48" height="48" rx="10" fill="#000" />
      <path
        fill="#fff"
        fillRule="evenodd"
        d="M32 12H16v24h16V12zm8 32H8V4h32v40z"
      />
    </svg>
  );
}

function LangChainIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg" {...props}>
      <rect width="48" height="48" rx="10" fill="#000" />
      <path
        fill="#7FC8FF"
        d="M15.062 31.952a15.068 15.068 0 0 0 0-21.302L4.412 0A15.074 15.074 0 0 0 0 10.652c0 3.992 1.588 7.826 4.412 10.65l10.65 10.65ZM37.348 32.938a15.07 15.07 0 0 0-21.3 0l10.65 10.65a15.072 15.072 0 0 0 21.302 0l-10.652-10.65ZM4.436 43.564a15.072 15.072 0 0 0 10.652 4.412V32.914H.024c0 3.992 1.59 7.828 4.412 10.65ZM41.46 17.19a15.068 15.068 0 0 0-21.302.002l10.65 10.652L41.46 17.19Z"
      />
    </svg>
  );
}

// Self-hosted mark (no hotlinked external image — self-contained deployment
// requirement): a simple winged-staff glyph evoking Hermes/Nous Research,
// drawn inline like every other icon in this file.
function HermesIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg" {...props}>
      <rect width="48" height="48" rx="10" fill="#111827" />
      <path
        d="M24 10v28M24 10c-5 0-8 3-8 3s3 3 8 3 8-3 8-3-3-3-8-3ZM17 30c2 3 5 4 7 4s5-1 7-4"
        stroke="#F5F5F4"
        strokeWidth="2.2"
        strokeLinecap="round"
        strokeLinejoin="round"
        fill="none"
      />
      <circle cx="24" cy="14" r="2" fill="#F5F5F4" />
    </svg>
  );
}

function GoogleChatIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg" {...props}>
      <path
        fill="#0F9D58"
        d="M45 16c0-5.5-4.5-10-10-10H13C7.5 6 3 10.5 3 16v12c0 5.5 4.5 10 10 10h7l4 8 4-8h7c5.5 0 10-4.5 10-10V16z"
      />
      <path fill="#fff" d="M14 20h20v2.5H14zm0 5h14v2.5H14z" />
    </svg>
  );
}

function OpenClawIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg" {...props}>
      <rect width="48" height="48" rx="10" fill="#111827" />
      <path
        d="M15 30c1.3 4.3 4.7 7 9 7s7.7-2.7 9-7M13 22c1.8-6.5 5.5-10 11-10s9.2 3.5 11 10M18 24h12M18 18h3M27 18h3"
        fill="none"
        stroke="#F8FAFC"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="3"
      />
      <path
        d="M13 22l-4-4M35 22l4-4M16 31l-4 4M32 31l4 4"
        fill="none"
        stroke="#38BDF8"
        strokeLinecap="round"
        strokeWidth="3"
      />
    </svg>
  );
}

function DifyIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" {...props}>
      <path d="M2 1H7.94339C11.8094 1 14.9434 4.13401 14.9434 8C14.9434 11.866 11.8094 15 7.9434 15H2V1Z" fill="white"/>
      <path d="M2 1H7.94339C11.8094 1 14.9434 4.13401 14.9434 8C14.9434 11.866 11.8094 15 7.9434 15H2V1Z" fill="url(#dify_angular_official)"/>
      <path d="M7.94336 8H8.20751V15H7.94336V8Z" fill="url(#dify_linear_official)"/>
      <defs>
        <radialGradient id="dify_angular_official" cx="0" cy="0" r="1" gradientUnits="userSpaceOnUse" gradientTransform="translate(7.9434 8) rotate(90) scale(8.75 8.75)">
          <stop stopColor="#001FC2"/>
          <stop offset="0.711334" stopColor="#0667F8" stopOpacity="0.2"/>
          <stop offset="1" stopColor="#155EEF" stopOpacity="0"/>
        </radialGradient>
        <linearGradient id="dify_linear_official" x1="8.06244" y1="8.43754" x2="7.93744" y2="9.20317" gradientUnits="userSpaceOnUse">
          <stop stopColor="white" stopOpacity="0"/>
          <stop offset="1" stopColor="white"/>
        </linearGradient>
      </defs>
    </svg>
  );
}

function LangGraphIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 488 488" fill="none" xmlns="http://www.w3.org/2000/svg" {...props}>
      <path d="M153.197 324.988C181.918 296.266 198.063 257.269 198.063 216.654C198.063 176.039 181.904 137.042 153.197 108.32L44.866 0C16.159 28.7218 0 67.7192 0 108.334C0 148.949 16.159 187.946 44.866 216.668L153.183 324.988H153.197Z" fill="currentColor"/>
      <path d="M379.871 335.012C351.164 306.304 312.153 290.145 271.554 290.145C230.954 290.145 191.944 306.304 163.223 335.012L271.554 443.346C300.261 472.054 339.271 488.213 379.885 488.213C420.498 488.213 459.495 472.054 488.215 443.346L379.885 335.012H379.871Z" fill="currentColor"/>
      <path d="M45.13 443.096C73.8509 471.804 112.847 487.963 153.461 487.963V334.762H0.25C0.263942 375.377 16.409 414.374 45.13 443.096Z" fill="currentColor"/>
      <path d="M421.695 174.84C392.974 146.132 353.978 129.959 313.35 129.973C272.737 129.973 233.74 146.132 205.02 174.854L313.35 283.188L421.695 174.84Z" fill="currentColor"/>
    </svg>
  );
}

function CrewAIIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg" {...props}>
      <g transform="translate(0.000000,48.000000) scale(0.100000,-0.100000)" fill="currentColor" stroke="none">
        <path d="M252 469 c-103 -22 -213 -172 -214 -294 -1 -107 60 -168 168 -167 130 1 276 133 234 211 -13 25 -27 26 -52 4 -31 -27 -32 -6 -4 56 34 77 33 103 -6 146 -38 40 -78 55 -126 44z m103 -40 c44 -39 46 -82 9 -163 -27 -60 -42 -68 -74 -36 -24 24 -26 67 -5 117 22 51 19 60 -11 32 -72 -65 -125 -189 -105 -242 9 -23 16 -27 53 -27 54 0 122 33 154 76 34 44 54 44 54 1 0 -75 -125 -167 -225 -167 -121 0 -181 92 -145 222 17 58 86 153 137 187 63 42 110 42 158 0z"/>
      </g>
    </svg>
  );
}

function A2AIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 199.067 198.437" xmlns="http://www.w3.org/2000/svg" {...props}>
      <path d="M181.924,127.367c3.138-8.975,4.856-18.61,4.856-28.642,0-47.977-39.032-87.009-87.009-87.009-10.102,0-19.804,1.737-28.832,4.916,1.652,2.336,3.018,4.812,4.092,7.382,7.783-2.584,16.1-3.987,24.74-3.987,43.394,0,78.698,35.304,78.698,78.698,0,8.637-1.405,16.95-3.988,24.732-4.417-1.745-9.225-2.714-14.262-2.714-21.455,0-38.847,17.393-38.847,38.847s17.393,38.847,38.847,38.847,38.847-17.393,38.847-38.847c0-13.416-6.801-25.243-17.143-32.223ZM183.236,184.307c-2.133.836-3.619.216-4.455-1.863l-1.936-4.697-2.438-5.914h-25.232l-2.325,5.914-1.846,4.697c-.837,2.106-2.296,2.727-4.375,1.863-2.16-.836-2.808-2.322-1.944-4.455l.855-2.105,4.991-12.29,13.431-33.07c.81-1.782,2.079-2.673,3.807-2.673h.162c1.755.081,2.942.972,3.564,2.673l18.493,44.78.24.58.869,2.105c.864,2.133.243,3.619-1.863,4.455Z" fill="#2874d7"/>
      <polygon points="46.001 43.115 48.811 43.115 38.726 18.572 29.046 43.115 32.747 43.115 46.001 43.115" fill="#2874d7"/>
      <path d="M99.771,177.422c-43.394,0-78.698-35.304-78.698-78.698,0-8.252,1.285-16.208,3.651-23.689,4.379,1.71,9.139,2.659,14.123,2.659,21.455,0,38.847-17.393,38.847-38.847S60.302,0,38.847,0,0,17.393,0,38.847c0,13.462,6.849,25.321,17.25,32.292-2.907,8.673-4.489,17.947-4.489,27.585,0,47.977,39.032,87.009,87.009,87.009,9.809,0,19.243-1.633,28.047-4.639-1.581-2.372-2.866-4.882-3.868-7.477-7.622,2.466-15.747,3.805-24.179,3.805ZM15.884,57.938L35.162,10.472c.81-1.782,2.079-2.673,3.807-2.673h.162c1.755.081,2.942.972,3.564,2.673l18.319,44.36,1.111,2.691.171.414c.864,2.133.243,3.619-1.863,4.455-2.133.836-3.619.216-4.455-1.863l-1.239-3.006-3.135-7.605h-25.232l-2.989,7.605-1.182,3.006c-.837,2.106-2.296,2.727-4.375,1.863-2.16-.836-2.808-2.322-1.944-4.455Z" fill="#2874d7"/>
      <path d="M72.381,126.727c15.113,15.113,39.616,15.113,54.729,0,15.113-15.113,15.113-39.616,0-54.729-15.113-15.113-39.616-15.113-54.729,0-15.113,15.113-15.113,39.616,0,54.729ZM85.081,73.609c2.43-1.876,5.873-2.815,10.328-2.815h8.666c5.076,0,8.842,1.229,11.3,3.685.657.657,1.219,1.415,1.701,2.26,1.318,2.313,1.985,5.321,1.985,9.039,0,3.241-.797,5.947-2.39,8.12-1.593,2.174-3.685,4.091-6.277,5.751-1.026.657-2.105,1.316-3.19,1.976-1.657,1.006-3.359,2.015-5.153,3.026-2.16,1.242-4.206,2.484-6.136,3.726-1.679,1.081-3.131,2.324-4.38,3.71-.187.208-.383.409-.56.624-1.364,1.648-2.262,3.713-2.693,6.197h26.73c1.428,0,2.405.452,2.934,1.352.311.529.468,1.211.468,2.05,0,2.269-1.134,3.402-3.402,3.402h-30.294c-2.322,0-3.484-1.134-3.484-3.402,0-.709.04-1.381.084-2.05.181-2.74.714-5.153,1.637-7.204,1.024-2.278,2.321-4.225,3.862-5.879.185-.199.359-.411.552-.601,1.796-1.768,3.706-3.266,5.731-4.495.262-.159.5-.299.757-.455,1.729-1.044,3.367-2.017,4.872-2.887,2.997-1.701,5.501-3.172,7.513-4.415.372-.23.709-.462,1.047-.694,1.486-1.02,2.654-2.057,3.468-3.113.999-1.296,1.499-2.876,1.499-4.739,0-.884-.048-1.681-.135-2.406-.229-1.912-.755-3.281-1.587-4.094-1.148-1.12-3.3-1.681-6.46-1.681h-8.666c-2.539,0-4.415.338-5.63,1.012-1.215.675-1.985,1.931-2.309,3.767-.046.264-.103.511-.17.744-.208.722-.515,1.298-.923,1.727-.54.567-1.363.851-2.47.851-1.134,0-1.998-.304-2.592-.911-.411-.42-.642-.982-.717-1.666-.034-.306-.046-.628-.012-.987.437-2.932,1.432-5.286,2.975-7.073.46-.533.963-1.021,1.521-1.453Z" fill="#2874d7"/>
      <polygon points="151.849 165.029 153.78 165.029 168.759 165.029 171.613 165.029 161.528 140.487 151.849 165.029" fill="#2874d7"/>
    </svg>
  );
}

function OpenAPIIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 504 360" xmlns="http://www.w3.org/2000/svg" {...props}>
      <g>
        <path fill="#93d500" d="M71.13,186.93h-29.56c0,.14,0,.29,.02,.43,.01,.28,.03,.56,.05,.83,0,.12,.01,.24,.02,.36,.02,.32,.05,.64,.08,.96,0,.07,.01,.14,.02,.22,.03,.36,.07,.71,.12,1.07,0,.04,0,.07,.01,.1,.05,.38,.1,.76,.15,1.14,0,0,0,0,0,.01,.34,2.36,.85,4.69,1.53,6.98,0,0,0,.02,0,.03,.11,.36,.22,.72,.33,1.08,0,.02,.01,.04,.02,.06s.01,.04,.02,.06c.1,.32,.21,.64,.33,.97,.03,.08,.06,.16,.09,.24,.1,.28,.2,.56,.3,.84,.05,.13,.1,.25,.14,.38,.09,.23,.18,.46,.27,.69,.07,.17,.14,.35,.21,.52,.07,.18,.15,.36,.23,.54,.09,.22,.19,.44,.29,.66,.06,.13,.12,.26,.18,.39,.12,.26,.24,.53,.37,.79,.04,.08,.08,.17,.12,.25,.14,.3,.29,.6,.44,.9,.03,.05,.05,.09,.07,.14,.17,.33,.34,.66,.52,.98,0,.01,.02,.03,.02,.04,.04,.07,.08,.13,.12,.2l25.24-15.21,.09-.06c-1-2.1-1.62-4.33-1.85-6.6Z"/>
        <path fill="#4d5a31" d="M78.39,200.5l-.07,.07-20.82,20.82c.11,.1,.21,.2,.32,.3,.19,.18,.39,.35,.59,.52,.1,.09,.2,.18,.3,.27,.24,.2,.47,.4,.71,.6,.06,.05,.13,.11,.19,.16,.27,.22,.54,.44,.81,.65,.03,.03,.07,.06,.1,.08,.3,.23,.59,.46,.89,.69,.01,0,.02,.02,.03,.02,1.25,.94,2.55,1.82,3.89,2.63,.05,.03,.09,.06,.14,.08,.25,.15,.51,.3,.77,.45,.16,.09,.31,.18,.47,.27,.15,.09,.3,.17,.45,.25,.27,.15,.54,.3,.81,.44,.04,.02,.07,.04,.11,.06,.76,.4,1.53,.76,2.3,1.12h0s.74-1.79,.74-1.79l10.47-25.43,.04-.09c-1.14-.61-2.24-1.33-3.27-2.18Z"/>
        <path fill="#6ba43a" d="M76.23,198.42c-.22-.25-.44-.51-.65-.77-.19-.23-.37-.46-.54-.7-.2-.27-.39-.54-.58-.82-.19-.28-.37-.56-.54-.84l-25.27,15.23c.39,.64,.79,1.27,1.21,1.89,.01,.02,.03,.04,.04,.07h0s0,.02,.01,.02c.01,.02,.03,.04,.04,.06,0,0,0,0,0,0,.03,.05,.07,.1,.1,.15,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,.44,.65,.89,1.29,1.35,1.91,.01,.01,.02,.03,.03,.04,0,.01,.02,.03,.03,.04,.2,.27,.4,.54,.61,.8,.03,.03,.05,.07,.08,.1,.21,.27,.43,.54,.65,.81,.05,.06,.11,.13,.16,.19,.2,.24,.4,.48,.6,.71,.09,.1,.18,.2,.27,.3,.17,.2,.35,.39,.52,.59,.1,.11,.2,.21,.3,.32,.05,.05,.09,.1,.13,.15,.13,.14,.26,.28,.39,.42,.05,.06,.11,.11,.16,.16,.23,.24,.46,.48,.7,.72l20.85-20.85c-.24-.24-.46-.48-.68-.73Z"/>
        <path fill="#4d5a31" d="M103.86,200.49c-.23,.19-.46,.37-.7,.54l.05,.09,15.2,25.23c.7-.46,1.4-.93,2.08-1.43,1.47-1.08,2.9-2.26,4.27-3.53l-20.82-20.82-.08-.08Z"/>
        <path fill="#93d500" d="M116.21,226.56l-.52-.86-14.19-23.56c-.29,.17-.58,.33-.87,.49-.3,.16-.59,.31-.9,.45-2.73,1.29-5.68,1.95-8.63,1.95-1.94,0-3.87-.28-5.74-.84-.32-.1-.63-.22-.94-.33-.31-.11-.63-.21-.94-.33l-10.46,25.41-.41,1-.35,.85c.25,.1,.5,.19,.74,.28,.31,.12,.62,.24,.93,.36,.16,.06,.32,.13,.48,.18,3.28,1.16,6.67,1.97,10.12,2.42,.13,.02,.27,.04,.4,.05,.14,.02,.28,.03,.42,.05,.27,.03,.53,.06,.8,.09,.07,0,.13,.01,.2,.02,.33,.03,.65,.06,.98,.08,.11,0,.22,.01,.33,.02,.29,.02,.57,.04,.86,.05,.18,0,.35,.01,.53,.02,.23,0,.45,.02,.68,.02,.33,0,.66,.01,.99,.01,.08,0,.16,0,.23,0,2.76,0,5.51-.23,8.23-.69c.29-.05,.58-.1,.86-.16,.16-.03,.33-.06,.49-.09,.17-.03,.34-.07,.51-.11,.28-.06,.56-.12,.84-.18,.05-.01,.1-.02,.15-.03,4.14-.97,8.14-2.46,11.9-4.44,.25-.13,.49-.28,.73-.41,.29-.16,.58-.32,.87-.49,.2-.12,.4-.22,.6-.34Z"/>
        <path fill="#4d5a31" d="M78.41,169.37c.23-.19,.46-.37,.7-.54l-.05-.09-15.2-25.23c-.71,.46-1.4,.94-2.09,1.44-1.47,1.08-2.89,2.26-4.26,3.52l20.82,20.82,.08,.08Z"/>
        <path fill="#4d5a31" d="M56.06,149.86c-.24,.24-.46,.48-.69,.72-.23,.24-.47,.48-.69,.72-1.54,1.67-2.94,3.41-4.21,5.22-.06,.09-.12,.17-.18,.26-.14,.21-.28,.42-.42,.62-.15,.22-.29,.44-.43,.66-.05,.08-.11,.16-.16,.24-4.79,7.51-7.36,16.03-7.7,24.63-.01,.33-.02,.67-.03,1,0,.33-.02,.67-.02,1h29.49c0-.33,.03-.67,.05-1,.02-.33,.02-.67,.05-1,.38-3.84,1.86-7.59,4.45-10.74,.21-.26,.45-.5,.67-.74,.22-.25,.43-.5,.67-.74l-20.85-20.85Z"/>
        <path fill="#4d5a31" d="M116.9,142.54c-.26-.16-.52-.31-.78-.47-.15-.09-.3-.17-.46-.26-.15-.09-.31-.17-.46-.26-.27-.15-.53-.29-.8-.43-.04-.02-.08-.04-.13-.07-1.73-.9-3.5-1.7-5.32-2.39-.05-.02-.09-.04-.14-.05-.39-.15-.79-.29-1.19-.43-3.22-1.12-6.55-1.91-9.94-2.36-.14-.02-.28-.04-.41-.06-.14-.02-.28-.03-.41-.05-.27-.03-.53-.06-.8-.09-.07,0-.15-.01-.22-.02-.32-.03-.64-.06-.95-.08-.12,0-.25-.02-.37-.02-.27-.02-.55-.04-.82-.05-.15,0-.29-.01-.43-.02v29.44c1.52,.16,3.02,.48,4.48,.97l21.75-21.75c-.81-.56-1.62-1.1-2.46-1.61Z"/>
        <path fill="#6ba43a" d="M90.14,135.35c-.33,0-.67,0-1,.02-2.09,.08-4.17,.3-6.23,.64-.05,0-.09,.01-.14,.02-.29,.05-.58,.1-.86,.16-.16,.03-.33,.06-.49,.09-.17,.03-.34,.07-.51,.11-.28,.06-.56,.12-.84,.18h0c-.05,.01-.1,.02-.15,.03-4.14,.97-8.14,2.46-11.9,4.44,.25,.13,.48,.28,.73,.41,.29,.16,.58,.33,.87,.49,.22,.12,.43,.24,.65,.37l14.71,24.41c.29-.17,.58-.33,.87-.49,.3-.16,.59-.31,.9-.45,2.1-1,4.33-1.62,6.6-1.85,.33-.03,.67-.06,1-.08,.33-.02,.67-.03,1-.03v-29.49c-.33,0-.67,.01-1,.02Z"/>
        <path fill="#4d5a31" d="M140.68,182.49c-.01-.26-.03-.53-.05-.79,0-.13-.02-.26-.03-.4-.02-.31-.05-.62-.08-.93,0-.08-.01-.16-.02-.24-.03-.35-.07-.7-.11-1.04-.05-.38-.1-.75-.15-1.12-.34-2.35-.85-4.68-1.52-6.97-.11-.36-.22-.71-.33-1.06-.02-.05-.03-.09-.04-.14-.1-.32-.21-.64-.32-.95-.03-.09-.06-.17-.09-.26-.1-.27-.2-.55-.3-.82-.05-.13-.1-.26-.15-.39-.09-.23-.18-.45-.27-.68-.07-.18-.14-.36-.22-.53-.07-.18-.15-.35-.22-.53-.1-.22-.2-.45-.29-.67-.06-.13-.11-.25-.17-.38-.12-.27-.25-.53-.37-.8-.04-.08-.08-.16-.12-.24-.15-.3-.3-.61-.45-.91-.02-.04-.04-.08-.07-.13-.17-.33-.34-.66-.52-.99-.86-1.58-1.8-3.11-2.82-4.58l-21.76,21.76c.5,1.46,.82,2.96,.97,4.48h29.56c0-.15,0-.29-.02-.44Z"/>
        <path fill="#6ba43a" d="M111.25,184.93c0,.33-.03,.67-.05,1-.02,.33-.02,.67-.05,1-.38,3.84-1.86,7.59-4.45,10.74-.21,.26-.45,.5-.67,.74-.22,.25-.43,.5-.67,.74l20.85,20.85c.24-.24,.46-.48,.69-.72,.23-.24,.47-.48,.69-.72,1.54-1.67,2.95-3.42,4.22-5.24,.05-.07,.1-.14,.15-.21,.16-.22,.31-.45,.46-.67,.13-.2,.27-.4,.4-.61,.06-.1,.13-.2,.19-.3,4.78-7.51,7.34-16.02,7.68-24.61,.01-.33,.02-.67,.03-1,0-.33,.02-.67,.02-1h-29.49Z"/>
      </g>
      <path fill="#424143" d="M149.5,126.57c-5.39-5.39-14.14-5.39-19.54,0-4.3,4.3-5.16,10.74-2.6,15.9l-30.09,30.09c-5.17-2.56-11.6-1.7-15.9,2.6-5.4,5.39-5.39,14.14,0,19.54,5.4,5.4,14.14,5.39,19.54,0,4.3-4.3,5.16-10.74,2.6-15.9l30.09-30.09c5.17,2.56,11.6,1.7,15.9-2.6,5.39-5.39,5.39-14.14,0-19.54Z"/>
      <g>
        <path fill="#424143" d="M155.91,184.53c0-14.78,10.81-25.37,25.67-25.37s25.59,10.59,25.59,25.37-10.81,25.37-25.59,25.37-25.67-10.59-25.67-25.37Zm40.52,0c0-9.19-5.81-16.11-14.86-16.11s-14.93,6.91-14.93,16.11,5.81,16.11,14.93,16.11,14.86-6.99,14.86-16.11Z"/>
        <path fill="#424143" d="M212.68,209.02v-49.06h22.95c10.66,0,16.47,7.21,16.47,15.81s-5.88,15.74-16.47,15.74h-12.5v17.5h-10.44Zm28.76-33.24c0-4.12-3.16-6.62-7.28-6.62h-11.03v13.16h11.03c4.12,0,7.28-2.5,7.28-6.55Z"/>
        <path fill="#424143" d="M256.81,209.02v-49.06h34.72v9.19h-24.27v10.3h23.76v9.19h-23.76v11.18h24.27v9.19h-34.72Z"/>
        <path fill="#424143" d="M331.68,209.02l-23.39-31.99v31.99h-10.45v-49.06h10.74l22.73,30.82v-30.82h10.44v49.06h-10.08Z"/>
        <path fill="#424143" d="M386.45,209.02l-3.09-8.31h-21.03l-3.09,8.31h-11.84l18.9-49.06h13.09l18.9,49.06h-11.84Zm-13.61-38.61l-7.65,21.11h15.3l-7.65-21.11Z"/>
        <path fill="#424143" d="M401.01,209.02v-49.06h22.95c10.66,0,16.47,7.21,16.47,15.81s-5.89,15.74-16.47,15.74h-12.5v17.5h-10.44Zm28.76-33.24c0-4.12-3.16-6.62-7.28-6.62h-11.03v13.16h11.03c4.12,0,7.28-2.5,7.28-6.55Z"/>
        <path fill="#424143" d="M445.14,209.02v-49.06h10.44v49.06h-10.44Z"/>
      </g>
    </svg>
  );
}

function AcpIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg" {...props}>
      <path d="M9.5,8H20.1a5,5,0,1,0,0-2H9.5a5.5,5.5,0,0,0,0,11h11a3.5,3.5,0,0,1,0,7H11.9a5,5,0,1,0,0,2h8.6a5.5,5.5,0,0,0,0-11H9.5a3.5,3.5,0,0,1,0-7ZM25,4a3,3,0,1,1-3,3A3,3,0,0,1,25,4ZM7,28a3,3,0,1,1,3-3A3,3,0,0,1,7,28Z" fill="currentColor"/>
    </svg>
  );
}

function FallbackIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" {...props}>
      <path d="M14 7h5a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2h-5M10 7H5a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2h5M8 12h8" />
    </svg>
  );
}

const ICONS: Record<string, (p: SVGProps<SVGSVGElement>) => ReactNode> = {
  a2a: A2AIcon,
  a2a_v1: A2AIcon,
  acp: AcpIcon,
  acp_legacy: AcpIcon,
  agent_to_agent: A2AIcon,
  anthropic: AnthropicIcon,
  bedrock_agent_core: BedrockAgentCoreIcon,
  claude: ClaudeIcon,
  codex: CodexIcon,
  crewai: CrewAIIcon,
  crewai_crew: CrewAIIcon,
  cursor: CursorIcon,
  dify: DifyIcon,
  dify_app: DifyIcon,
  elastic: ElasticIcon,
  gemini: GeminiIcon,
  gemini_antigravity: GeminiIcon,
  gmail: GmailIcon,
  hermes: HermesIcon,
  langchain: LangChainIcon,
  langgraph: LangGraphIcon,
  langgraph_assistant: LangGraphIcon,
  linear: LinearIcon,
  openclaw: OpenClawIcon,
  opencode: OpenCodeIcon,
  openapi: OpenAPIIcon,
  openapi_rest: OpenAPIIcon,
  pylon: PylonIcon,
  google_chat: GoogleChatIcon,
  slack: SlackIcon,
  teams: TeamsIcon,
};

export function BrandIcon({
  id,
  className,
}: {
  id: string;
  className?: string;
}) {
  const Icon = ICONS[id] ?? FallbackIcon;
  return <Icon className={className} />;
}
