import {
  HeadContent,
  Outlet,
  createRootRouteWithContext,
} from "@tanstack/react-router";
import { TanStackRouterDevtools } from "@tanstack/react-router-devtools";
import { Toaster } from "@tessera/ui/components/sonner";
import { useEffect, useSyncExternalStore } from "react";

import Header from "@/components/header";
import { ThemeProvider } from "@/components/theme-provider";
import {
  applyDocumentLocale,
  getLocale,
  subscribeToLocale,
} from "@/lib/locale";

import "../index.css";

/**
 * Nothing is threaded through the router yet. Spelt this way rather than as an
 * empty interface, which the linter refuses because `{}` means "any non-null
 * value" and not "an object with no properties".
 */
export type RouterAppContext = Record<string, never>;

export const Route = createRootRouteWithContext<RouterAppContext>()({
  component: RootComponent,
  head: () => ({
    links: [
      {
        href: "/favicon.ico",
        rel: "icon",
      },
    ],
    meta: [
      {
        title: "my-better-t-app",
      },
      {
        content: "my-better-t-app is a web application",
        name: "description",
      },
    ],
  }),
});

function RootComponent() {
  const locale = useSyncExternalStore(subscribeToLocale, getLocale);

  useEffect(() => {
    applyDocumentLocale(locale);
  }, [locale]);

  return (
    <>
      <HeadContent />
      <ThemeProvider
        attribute="class"
        defaultTheme="dark"
        disableTransitionOnChange
        storageKey="vite-ui-theme"
      >
        {/*
          Message functions read the locale when they are called, so nothing
          re-renders on its own when the prefix in the URL changes. Keying the
          tree is the blunt but reliable answer; the cost is that a locale
          switch remounts, so anything half-typed in a form is lost. That is an
          explicit, rare action, and the alternative — every component that
          renders text subscribing to the locale — spreads through the whole
          app to buy something nobody asked for.
        */}
        <div className="grid h-svh grid-rows-[auto_1fr]" key={locale}>
          <Header />
          <Outlet />
        </div>
        <Toaster richColors />
      </ThemeProvider>
      <TanStackRouterDevtools position="bottom-left" />
    </>
  );
}
