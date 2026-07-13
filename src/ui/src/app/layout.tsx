import type { Metadata } from "next";
import { Toaster } from "sonner";
import { ThemeProvider } from "@/components/theme-provider";
import "./globals.css";

// Self-contained deployment: no Google Fonts fetch. Fonts are the system
// stack defined directly in globals.css.

export const metadata: Metadata = {
  title: "OAP 开放智能体平台",
  description: "OAP (Open Agent Platform) console",
};

// Runs synchronously before React hydration — reads ?token= from URL,
// stores in sessionStorage, strips the param. Guarantees the key is
// available before any component useEffect or API call fires.
const tokenBootstrap = `(function(){
  try {
    var p = new URLSearchParams(window.location.search);
    var t = p.get('token');
    if (t) {
      sessionStorage.setItem('lite-harness-master-key', t);
      p.delete('token');
      var qs = p.toString();
      history.replaceState(null, '', location.pathname + (qs ? '?' + qs : ''));
    }
  } catch(e) {}
})();`;

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en" suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: tokenBootstrap }} />
      </head>
      <body className="antialiased">
        <ThemeProvider
          attribute="class"
          defaultTheme="light"
          enableSystem
          disableTransitionOnChange
        >
          <a
            href="#main-content"
            className="sr-only focus:not-sr-only focus:fixed focus:left-4 focus:top-4 focus:z-50 focus:rounded focus:bg-background focus:px-4 focus:py-2 focus:text-sm focus:font-medium focus:ring-2 focus:ring-ring"
          >
            Skip to content
          </a>
          {children}
          <Toaster />
        </ThemeProvider>
      </body>
    </html>
  );
}
