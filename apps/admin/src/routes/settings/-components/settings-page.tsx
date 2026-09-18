import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import type * as React from "react";

import { AdminShell } from "@/components/admin-shell";

/** 每個設定分頁共用的外框。分頁之間只差標題、說明跟裡面那張表單。 */
export function SettingsPage({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: React.ReactNode;
}) {
  return (
    <AdminShell title="系統設定">
      <SettingsCard description={description} title={title}>
        {children}
      </SettingsCard>
    </AdminShell>
  );
}

/**
 * 單獨一張卡。給一頁裡有好幾個各自送出的表單用 —— 登入方式那頁的三個 provider
 * 各存各的，混成一批會讓其中一個的錯誤把另外兩個的改動也退回。
 */
export function SettingsCard({
  title,
  description,
  children,
}: {
  title: string;
  description: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <Card className="ring-foreground/10">
      <CardHeader>
        <CardTitle>{title}</CardTitle>
        <CardDescription>{description}</CardDescription>
      </CardHeader>
      <CardContent>{children}</CardContent>
    </Card>
  );
}
