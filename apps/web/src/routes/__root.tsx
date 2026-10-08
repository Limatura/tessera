import type { QueryClient } from "@tanstack/react-query";
import {
  HeadContent,
  Outlet,
  createRootRouteWithContext,
} from "@tanstack/react-router";
import { TanStackRouterDevtools } from "@tanstack/react-router-devtools";
import { ThemeProvider } from "@tessera/ui/components/theme-provider";
import { Toaster } from "@tessera/ui/components/toast";
import { useEffect, useSyncExternalStore } from "react";

import { AppearanceSync } from "@/components/appearance-sync";
import { readSiteAppearance } from "@/lib/appearance";
import {
  applyDocumentLocale,
  getLocale,
  subscribeToLocale,
} from "@/lib/locale";
import { requirePasswordChanged } from "@/lib/route-guard";
import * as m from "@/paraglide/messages";

import "../index.css";

/**
 * Carries the QueryClient so route `beforeLoad`/`loader` hooks can read or
 * prefetch query state — e.g. the session query from `@tessera/auth/hooks`, to
 * gate a route on `isAuthenticated` without a separate auth store.
 */
export interface RouterAppContext {
  queryClient: QueryClient;
}

export const Route = createRootRouteWithContext<RouterAppContext>()({
  beforeLoad: ({ context, location }) =>
    requirePasswordChanged(context.queryClient, location),
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
        title: m.app_name(),
      },
      {
        content: m.app_description(),
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
      {/*
        `defaultTheme` 讀 `<html data-mode>` —— 伺服器把站台預設寫在那裡。寫死
        "dark" 會讓擁有者設的預設明暗對沒選過的人失效,而那正是這個設定的用途。
      */}
      <ThemeProvider
        attribute="class"
        defaultTheme={readSiteAppearance().mode}
        disableTransitionOnChange
        storageKey="theme"
      >
        <AppearanceSync />
        {/*
          消息函数是在被调用当下读取 locale 的，locale 变了它们不会自己重渲染。
          用 key={locale} 强制整棵树 remount 是最直接可靠的做法；代价是切换语言
          时任何没提交的表单内容会被清空 —— 这是一个明确、不常发生的操作，可以接受。
        */}
        {/*
          grid-cols-1 是 minmax(0, 1fr):沒寫的話隱含欄是 auto,會被內容的最小寬度撐開,
          頁面裡任何 overflow-x-auto 都等不到捲動的機會,整頁直接變寬(#379)。
        */}
        <div
          className="grid h-svh grid-cols-1 grid-rows-[auto_1fr]"
          key={locale}
        >
          <Outlet />
        </div>
        <Toaster />
      </ThemeProvider>
      <TanStackRouterDevtools position="bottom-left" />
    </>
  );
}
