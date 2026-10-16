import { IconChevronLeft, IconChevronRight } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";
import { buttonVariants } from "@tessera/ui/components/button";
import { cn } from "@tessera/ui/lib/utils";

import * as m from "@/paraglide/messages";

type PagedRoute = "/admin/all-shares" | "/admin/users";

/**
 * Previous/next are `Link`s, not buttons, so middle-click and copying the link
 * work. At either end they render as a disabled span: a link has no disabled
 * state, the only option is to drop its href.
 *
 * Hidden when there is one page. `total` is the full count, not this page's length.
 *
 * A phone has no room for the range sentence and two labelled buttons, so it
 * gets "‹ 3 / 12 ›" with icon buttons large enough to tap.
 */
export function Pager({
  to,
  page,
  pageSize,
  total,
}: {
  to: PagedRoute;
  page: number;
  pageSize: number;
  total: number;
}) {
  const lastPage = Math.max(1, Math.ceil(total / pageSize));

  if (lastPage === 1) {
    return null;
  }

  const offset = (page - 1) * pageSize;

  return (
    <div className="mt-4 flex items-center justify-between gap-4">
      <p className="hidden text-muted-foreground text-sm tabular-nums md:block">
        {m.pager_range({
          from: Math.min(offset + 1, total),
          to: Math.min(offset + pageSize, total),
          total,
        })}
      </p>
      <div className="flex w-full items-center justify-between gap-2 md:w-auto md:justify-end">
        <PageLink
          disabled={page <= 1}
          label={m.pager_previous()}
          page={page - 1}
          to={to}
        >
          <IconChevronLeft size={16} />
          <span className="max-md:sr-only">{m.pager_previous()}</span>
        </PageLink>
        <span className="text-muted-foreground text-sm tabular-nums md:hidden">
          {page} / {lastPage}
        </span>
        <PageLink
          disabled={page >= lastPage}
          label={m.pager_next()}
          page={page + 1}
          to={to}
        >
          <span className="max-md:sr-only">{m.pager_next()}</span>
          <IconChevronRight size={16} />
        </PageLink>
      </div>
    </div>
  );
}

function PageLink({
  to,
  page,
  disabled,
  label,
  children,
}: {
  to: PagedRoute;
  page: number;
  disabled: boolean;
  label: string;
  children: React.ReactNode;
}) {
  const className = cn(
    buttonVariants({ size: "sm", variant: "outline" }),
    "max-md:size-10 max-md:px-0",
    disabled && "pointer-events-none opacity-50"
  );

  if (disabled) {
    return (
      <span aria-disabled className={className}>
        {children}
      </span>
    );
  }

  return (
    // Keep other search params (such as `deleted` on the users page) and change only the page.
    <Link
      aria-label={label}
      className={className}
      search={(previous) => ({ ...previous, page })}
      to={to}
    >
      {children}
    </Link>
  );
}
