import type * as React from "react";

/**
 * Content area of a settings tab: a title, one sentence of description, then
 * one or more `SettingsCard`s.
 *
 * The frame (sidebar, grouped menu) and width live in `settings/route.tsx`;
 * this covers only the page's own part.
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
 * One section of a page. With several, each covers one thing: the title says
 * what it controls, the description is one sentence at most.
 *
 * Also used for a page with several independently submitted forms: the three
 * providers on the sign-in page save separately, since in one batch an error in
 * one would send back the changes to the other two.
 */
export function SettingsCard({
  title,
  description,
  action,
  children,
}: {
  title: string;
  description?: React.ReactNode;
  /** Buttons on the right of the title row, such as "Add". */
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
