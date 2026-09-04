import type * as React from "react";

/** 登入後那幾頁共用的頁首:左邊一個圖示,右邊標題跟說明。 */
export function PageHeader({
  icon,
  title,
  description,
}: {
  icon: React.ReactNode;
  title: string;
  description?: string;
}) {
  return (
    <header className="flex items-center gap-3">
      <span className="flex size-10 items-center justify-center rounded-lg border bg-muted/40 text-muted-foreground">
        {icon}
      </span>
      <div>
        <h1 className="font-semibold text-xl">{title}</h1>
        {description ? (
          <p className="text-muted-foreground text-sm">{description}</p>
        ) : null}
      </div>
    </header>
  );
}

/**
 * 登入後內容頁的外框。
 *
 * pt-20:Header 是 fixed 的,內容要自己讓開它的高度。
 */
export function PageShell({ children }: { children: React.ReactNode }) {
  return (
    <div className="mx-auto w-full max-w-5xl px-4 pt-20 pb-16">{children}</div>
  );
}
