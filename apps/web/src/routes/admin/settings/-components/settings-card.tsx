import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import type * as React from "react";

/**
 * 設定分頁的一張卡：標題、說明、裡面那張表單。
 *
 * 一頁通常只有一張；一頁裡有好幾個各自送出的表單時就放好幾張 —— 登入方式那頁的三個
 * provider 各存各的，混成一批會讓其中一個的錯誤把另外兩個的改動也退回。
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
