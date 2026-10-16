import { IconRefresh } from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";
import { Skeleton } from "@tessera/ui/components/skeleton";

import * as m from "@/paraglide/messages";

/**
 * Placeholder rows while an admin list loads, shaped like what replaces them so
 * the page does not jump: two-line cards on a phone, table rows from `md` up.
 */
export function ListSkeleton({ rows = 5 }: { rows?: number }) {
  const keys = Array.from({ length: rows }, (_, index) => `row-${index}`);

  return (
    <output aria-busy="true" aria-label={m.common_loading()} className="block">
      <ul className="flex flex-col gap-2 md:hidden">
        {keys.map((key) => (
          <li className="flex flex-col gap-2 rounded-lg border p-3" key={key}>
            <div className="flex items-center justify-between gap-3">
              <Skeleton className="h-4 w-2/5" />
              <Skeleton className="h-5 w-14 rounded-full" />
            </div>
            <Skeleton className="h-3 w-3/5" />
          </li>
        ))}
      </ul>
      <div className="hidden flex-col md:flex">
        <Skeleton className="mb-1 h-8 w-full" />
        {keys.map((key) => (
          <div className="flex items-center gap-6 border-b py-3" key={key}>
            <Skeleton className="h-4 w-1/5" />
            <Skeleton className="h-4 w-1/4" />
            <Skeleton className="h-5 w-16 rounded-full" />
            <Skeleton className="ml-auto h-4 w-10" />
          </div>
        ))}
      </div>
    </output>
  );
}

/** A failed load says so and offers another try instead of leaving a dead card. */
export function ListError({
  message,
  onRetry,
  isRetrying,
}: {
  message: string;
  onRetry: () => void;
  isRetrying: boolean;
}) {
  return (
    <div className="flex flex-col items-center gap-3 py-10 text-center">
      <p className="text-destructive text-sm" role="alert">
        {message}
      </p>
      <Button
        disabled={isRetrying}
        onClick={onRetry}
        size="sm"
        type="button"
        variant="outline"
      >
        <IconRefresh className={isRetrying ? "animate-spin" : undefined} />
        {m.admin_retry()}
      </Button>
    </div>
  );
}

export function ListEmpty({ children }: { children: React.ReactNode }) {
  return (
    <p className="py-10 text-center text-muted-foreground text-sm">
      {children}
    </p>
  );
}
