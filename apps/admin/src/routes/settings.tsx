import { createRoute } from "@tanstack/react-router";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import { SidebarInset, SidebarProvider } from "@tessera/ui/components/sidebar";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@tessera/ui/components/tabs";
import type * as React from "react";

import { AppSidebar } from "@/components/app-sidebar";
import { OidcSettingsForm } from "@/components/settings/oidc-settings-form";
import { SessionSettingsForm } from "@/components/settings/session-settings-form";
import { SiteSettingsForm } from "@/components/settings/site-settings-form";
import { StorageSettingsForm } from "@/components/settings/storage-settings-form";
import { ThemeSettingsForm } from "@/components/settings/theme-settings-form";
import { UploadSettingsForm } from "@/components/settings/upload-settings-form";
import { SiteHeader } from "@/components/site-header";

import { Route as rootRoute } from "./__root";

export const Route = createRoute({
  component: SettingsComponent,
  getParentRoute: () => rootRoute,
  path: "/settings",
});

const TABS: {
  value: string;
  label: string;
  description: string;
  content: React.ReactNode;
}[] = [
  {
    content: <SiteSettingsForm />,
    description: "網頁標題、敘述與 SEO 關鍵詞",
    label: "網站資訊",
    value: "site",
  },
  {
    content: <OidcSettingsForm />,
    description: "透過外部身分提供者登入",
    label: "OIDC 設定",
    value: "oidc",
  },
  {
    content: <ThemeSettingsForm />,
    description: "介面預設主題與主色調",
    label: "介面主題",
    value: "theme",
  },
  {
    content: <StorageSettingsForm />,
    description: "儲存後端位址與分片上傳大小",
    label: "存儲配置",
    value: "storage",
  },
  {
    content: <UploadSettingsForm />,
    description: "上傳限流、大小上限與保存時間",
    label: "上傳設置",
    value: "upload",
  },
  {
    content: <SessionSettingsForm />,
    description: "登入會話時長",
    label: "登入會話",
    value: "session",
  },
];

function SettingsComponent() {
  return (
    <SidebarProvider
      style={
        {
          "--header-height": "calc(var(--spacing) * 12)",
          "--sidebar-width": "calc(var(--spacing) * 72)",
        } as React.CSSProperties
      }
    >
      <AppSidebar variant="inset" />
      <SidebarInset>
        <SiteHeader title="系統設定" />
        <div className="flex flex-1 flex-col">
          <div className="@container/main flex flex-1 flex-col gap-6 px-4 py-4 md:py-6 lg:px-6">
            <Tabs defaultValue={TABS[0].value}>
              <TabsList>
                {TABS.map((tab) => (
                  <TabsTrigger key={tab.value} value={tab.value}>
                    {tab.label}
                  </TabsTrigger>
                ))}
              </TabsList>
              {TABS.map((tab) => (
                <TabsContent key={tab.value} value={tab.value}>
                  <Card className="ring-foreground/10">
                    <CardHeader>
                      <CardTitle>{tab.label}</CardTitle>
                      <CardDescription>{tab.description}</CardDescription>
                    </CardHeader>
                    <CardContent>{tab.content}</CardContent>
                  </Card>
                </TabsContent>
              ))}
            </Tabs>
          </div>
        </div>
      </SidebarInset>
    </SidebarProvider>
  );
}
