import { PHASE_DEVELOPMENT_SERVER } from "next/constants.js";
import { fileURLToPath } from "node:url";

// Build-time Chinese localization codemod (see i18n/). Applied as a webpack
// "pre" loader so it rewrites our raw source before SWC compiles it; the
// .ts/.tsx files on disk are never modified, keeping merge conflicts with the
// upstream open-source repo near zero.
const zhLoader = fileURLToPath(new URL("./i18n/zh-loader.cjs", import.meta.url));
const srcDir = fileURLToPath(new URL("./src", import.meta.url));

export default function nextConfig(phase) {
  const apiBase = process.env.LITELLM_DEV_API_BASE?.replace(/\/+$/, "");
  const isDev = phase === PHASE_DEVELOPMENT_SERVER;
  return {
    output: isDev ? undefined : "export",
    compress: !isDev,
    trailingSlash: !isDev,
    images: { unoptimized: true },
    allowedDevOrigins: ["127.0.0.1"],
    webpack(config) {
      config.module.rules.push({
        test: /\.(tsx|ts)$/,
        include: srcDir,
        exclude: /node_modules/,
        enforce: "pre",
        use: [{ loader: zhLoader }],
      });
      return config;
    },
    ...(isDev && apiBase
      ? {
          async rewrites() {
            return [
              { source: "/api/:path*", destination: `${apiBase}/api/:path*` },
              { source: "/v1/:path*", destination: `${apiBase}/v1/:path*` },
              { source: "/public/:path*", destination: `${apiBase}/public/:path*` },
              { source: "/session/:path*", destination: `${apiBase}/session/:path*` },
              { source: "/event", destination: `${apiBase}/event` },
              { source: "/whoami", destination: `${apiBase}/whoami` },
              { source: "/:server/mcp", destination: `${apiBase}/:server/mcp` },
            ];
          },
        }
      : {}),
  };
}
