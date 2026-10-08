import type * as React from "react";

/**
 * 每個設定分頁的內容區：標題、一句說明，底下是一張或幾張 `SettingsCard`。
 *
 * 外框（側欄、分組選單）跟寬度都在 `settings/route.tsx`，這裡只管這一頁自己的部分。
 */
export function SettingsPage({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-6">
      <header className="flex flex-col gap-1">
        <h2 className="font-heading font-semibold text-xl tracking-tight">
          {title}
        </h2>
        {description && (
          <p className="text-muted-foreground text-sm">{description}</p>
        )}
      </header>
      {children}
    </div>
  );
}

/**
 * 頁面裡的一個區塊。一頁多個區塊時，每塊各講一件事：標題說它管什麼，說明最多一句。
 *
 * 也給「一頁有好幾個各自送出的表單」用 —— 登入方式那頁的三個 provider 各存各的，
 * 混成一批會讓其中一個的錯誤把另外兩個的改動也退回。
 */
export function SettingsCard({
  title,
  description,
  action,
  children,
}: {
  title: string;
  description?: React.ReactNode;
  /** 標題列右邊的按鈕，例如「新增」。 */
  action?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section className="rounded-xl border bg-card text-card-foreground">
      <header className="flex items-start justify-between gap-4 border-b px-5 py-4">
        <div className="flex min-w-0 flex-col gap-1">
          <h3 className="font-medium text-base">{title}</h3>
          {description && (
            <p className="text-muted-foreground text-sm">{description}</p>
          )}
        </div>
        {action && <div className="shrink-0">{action}</div>}
      </header>
      <div className="px-5 py-5">{children}</div>
    </section>
  );
}
