import {
  HeadContent,
  Outlet,
  createRootRouteWithContext,
} from "@tanstack/react-router";
import { TanStackRouterDevtools } from "@tanstack/react-router-devtools";
import { I18nProvider } from "@tessera/i18n";
import { Toaster } from "@tessera/ui/components/sonner";

import Header from "@/components/header";
import { ThemeProvider } from "@/components/theme-provider";
import { adminDictionaries } from "@/i18n/dictionaries";

import "../index.css";

export interface RouterAppContext {}

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
  return (
    <>
      <HeadContent />
      <I18nProvider dictionaries={adminDictionaries}>
        <ThemeProvider
          attribute="class"
          defaultTheme="dark"
          disableTransitionOnChange
          storageKey="vite-ui-theme"
        >
          <div className="grid h-svh grid-rows-[auto_1fr]">
            <Header />
            <Outlet />
          </div>
          <Toaster richColors />
        </ThemeProvider>
      </I18nProvider>
      <TanStackRouterDevtools position="bottom-left" />
    </>
  );
}
