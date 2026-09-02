import type { QueryClient } from "@tanstack/react-query";
import {
  HeadContent,
  Outlet,
  createRootRouteWithContext,
} from "@tanstack/react-router";
import { TanStackRouterDevtools } from "@tanstack/react-router-devtools";
import { Toaster } from "@tessera/ui/components/toast";
import { useEffect, useSyncExternalStore } from "react";

import { ThemeProvider } from "@/components/theme-provider";
import {
  applyDocumentLocale,
  getLocale,
  subscribeToLocale,
} from "@/lib/locale";

import "../index.css";

export interface RouterAppContext {
  queryClient: QueryClient;
}

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
          消息函数是在被调用当下读取 locale 的，locale 变了它们不会自己重渲染。
          用 key={locale} 强制整棵树 remount 是最直接可靠的做法；代价是切换语言
          时任何没提交的表单内容会被清空 —— 这是一个明确、不常发生的操作，可以接受。
        */}
        <div className="grid h-svh grid-rows-[auto_1fr]" key={locale}>
          <Outlet />
        </div>
        <Toaster />
      </ThemeProvider>
      <TanStackRouterDevtools position="bottom-left" />
    </>
  );
}
