import "@/assets/main.css";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ThemeProvider } from "@tessera/ui/components/theme-provider";
import { Toaster } from "@tessera/ui/components/toast";
import { StrictMode } from "react";
import type { ComponentType, ReactNode } from "react";
import { createRoot } from "react-dom/client";

import { browser } from "#imports";

/** Renders an extension page: its `#root`, in the user's language and theme. */
export function mount(App: ComponentType): void {
  const root = document.querySelector("#root");
  if (!root) {
    throw new Error("the page has no #root; its index.html may be broken");
  }
  document.documentElement.lang = browser.i18n.getUILanguage();
  createRoot(root).render(
    <StrictMode>
      <Providers>
        <App />
      </Providers>
    </StrictMode>
  );
}

/** One per page: each extension page mounts once. */
const queryClient = new QueryClient({
  defaultOptions: {
    // A 401 or a refused request is an answer to show, not a blip to retry.
    queries: { retry: false, staleTime: 30_000 },
  },
});

function Providers({ children }: { children: ReactNode }) {
  return (
    <ThemeProvider
      attribute="class"
      defaultTheme="system"
      disableTransitionOnChange
      enableSystem
    >
      <QueryClientProvider client={queryClient}>
        {children}
        <Toaster />
      </QueryClientProvider>
    </ThemeProvider>
  );
}
