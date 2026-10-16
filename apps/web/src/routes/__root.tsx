import type { QueryClient } from "@tanstack/react-query";
import {
  HeadContent,
  Outlet,
  createRootRouteWithContext,
} from "@tanstack/react-router";
import { ThemeProvider } from "@tessera/ui/components/theme-provider";
import { Toaster } from "@tessera/ui/components/toast";
import { lazy, Suspense, useEffect, useSyncExternalStore } from "react";

import { AppearanceSync } from "@/components/appearance-sync";
import { readSiteAppearance } from "@/lib/appearance";
import {
  applyDocumentLocale,
  getLocale,
  subscribeToLocale,
} from "@/lib/locale";
import { requirePasswordChanged } from "@/lib/route-guard";

import "../index.css";

/**
 * Devtools are needed only in development and should take no space in the production
 * bundle.
 */
const TanStackRouterDevtools = import.meta.env.PROD
  ? null
  : lazy(async () => {
      const mod = await import("@tanstack/react-router-devtools");
      return { default: mod.TanStackRouterDevtools };
    });

/**
 * Carries the QueryClient so route `beforeLoad`/`loader` hooks can read or
 * prefetch query state (e.g. the session query from `@tessera/auth/hooks`), to
 * gate a route on `isAuthenticated` without a separate auth store.
 */
export interface RouterAppContext {
  queryClient: QueryClient;
}

export const Route = createRootRouteWithContext<RouterAppContext>()({
  beforeLoad: ({ context, location }) =>
    requirePasswordChanged(context.queryClient, location),
  component: RootComponent,
});

function RootComponent() {
  const locale = useSyncExternalStore(subscribeToLocale, getLocale);

  useEffect(() => {
    applyDocumentLocale(locale);
  }, [locale]);

  return (
    <>
      <HeadContent />
      {/*
        `defaultTheme` reads `<html data-mode>`, where the server writes the
        site default. Hard-coding "dark" would ignore the owner's default for
        everyone who never chose, and that default is what the setting is for.
      */}
      <ThemeProvider
        attribute="class"
        defaultTheme={readSiteAppearance().mode}
        disableTransitionOnChange
        storageKey="theme"
      >
        <AppearanceSync />
        {/*
          Message functions read the locale when called and do not re-render
          when it changes. key={locale} remounts the tree, the simplest reliable
          fix; the cost is that a language switch clears unsubmitted forms, an
          explicit and rare action.
        */}
        {/*
          grid-cols-1 is minmax(0, 1fr). Without it the implicit column is auto
          and grows to the content's minimum width, so no overflow-x-auto inside
          ever scrolls and the page itself widens instead.
        */}
        <div
          className="grid h-svh grid-cols-1 grid-rows-[auto_1fr]"
          key={locale}
        >
          <Outlet />
        </div>
        <Toaster />
      </ThemeProvider>
      {TanStackRouterDevtools ? (
        <Suspense fallback={null}>
          <TanStackRouterDevtools position="bottom-left" />
        </Suspense>
      ) : null}
    </>
  );
}
