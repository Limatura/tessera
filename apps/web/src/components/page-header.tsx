import { IconArrowLeft } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";
import { buttonVariants } from "@tessera/ui/components/button";
import type * as React from "react";

import * as m from "@/paraglide/messages";

/**
 * Shared header for the signed-in pages: an icon, title and description, and a
 * way back to the upload page on the right. These pages are reached from the
 * account menu, and the logo alone does not say it leads home.
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
      <span className="flex size-10 shrink-0 items-center justify-center rounded-lg border bg-muted/40 text-muted-foreground">
        {icon}
      </span>
      <div className="min-w-0 flex-1">
        <h1 className="font-semibold text-xl">{title}</h1>
        {description ? (
          <p className="text-muted-foreground text-sm">{description}</p>
        ) : null}
      </div>
      <Link
        aria-label={m.share_back_home()}
        className={buttonVariants({
          className: "shrink-0",
          size: "sm",
          variant: "outline",
        })}
        to="/"
      >
        <IconArrowLeft className="size-4" />
        <span className="max-sm:hidden">{m.share_back_home()}</span>
      </Link>
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
