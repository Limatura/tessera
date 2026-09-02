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
      <Card className="ring-foreground/10">
        <CardHeader>
          <CardTitle>{title}</CardTitle>
          <CardDescription>{description}</CardDescription>
        </CardHeader>
        <CardContent>{children}</CardContent>
      </Card>
    </AdminShell>
  );
}
