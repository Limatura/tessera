import type * as React from "react";

/**
 * Shared header for the signed-in pages: an icon on the left, title and
 * description on the right.
 */
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
 * Frame for signed-in content pages.
 *
 * pt-20: the Header is fixed, so content has to clear its height itself.
 */
export function PageShell({ children }: { children: React.ReactNode }) {
  return (
    <div className="mx-auto w-full max-w-5xl px-4 pt-20 pb-16">{children}</div>
  );
}
