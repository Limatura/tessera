import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { z } from "zod";

import { useLogFeed } from "@/hooks/admin/use-log-feed";
import type { LogFilters } from "@/hooks/admin/use-log-feed";
import { LOG_LEVELS } from "@/lib/admin/log-entries";
import type { LogEntry } from "@/lib/admin/log-entries";
import { PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

import { LogDetailSheet } from "./-components/log-detail-sheet";
import { LogList } from "./-components/log-list";
import { LogToolbar } from "./-components/log-toolbar";

/** The filters live in the URL, so a view of the log can be sent to someone. */
const logsSearchSchema = z.object({
  // oxlint-disable-next-line promise/prefer-await-to-then -- zod's catch, not a Promise's
  level: z.enum(LOG_LEVELS).optional().catch("info"),
  // oxlint-disable-next-line promise/prefer-await-to-then -- zod's catch, not a Promise's
  q: z.string().optional().catch(""),
  // oxlint-disable-next-line promise/prefer-await-to-then -- zod's catch, not a Promise's
  since: z.iso.datetime().optional().catch(""),
  // oxlint-disable-next-line promise/prefer-await-to-then -- zod's catch, not a Promise's
  until: z.iso.datetime().optional().catch(""),
});

export const Route = createFileRoute("/admin/logs/")({
  beforeLoad: ({ context, location }) =>
    requirePermission(context.queryClient, location.href, PERMISSIONS.logsRead),
  component: LogsComponent,
  staticData: { title: m.logs_page_title },
  validateSearch: logsSearchSchema,
});

function LogsComponent() {
  const search = Route.useSearch();
  const navigate = useNavigate({ from: Route.fullPath });
  // Pausing is a display choice, so it is not in the URL or the server's business.
  const [paused, setPaused] = useState(false);
  // At the top of the list, where new lines appear. Scrolled down, they wait.
  const [pinned, setPinned] = useState(true);
  const [selected, setSelected] = useState<LogEntry | null>(null);

  const filters: LogFilters = {
    level: search.level ?? null,
    q: search.q ?? "",
    since: search.since ?? null,
    until: search.until ?? null,
  };
  const feed = useLogFeed(filters, paused || !pinned);

  const handleFiltersChange = (next: Partial<LogFilters>) => {
    const merged = { ...filters, ...next };
    navigate({
      replace: true,
      search: {
        level: merged.level ?? undefined,
        q: merged.q || undefined,
        since: merged.since ?? undefined,
        until: merged.until ?? undefined,
      },
    });
  };

  // The admin frame is `h-svh` and its main area scrolls, so this page must fit
  // inside it exactly or the toolbar scrolls away with the overflow. Below `md`
  // the content has `py-4` (2rem in all); from `md` up, `py-6` (3rem) plus the
  // inset's `m-2` (1rem). The title bar's `--header-height` comes off both.
  return (
    <div className="flex h-[calc(100svh-var(--header-height)-2rem)] min-h-96 flex-col gap-3 md:h-[calc(100svh-var(--header-height)-4rem)]">
      <LogToolbar
        feed={feed}
        filters={filters}
        onFiltersChange={handleFiltersChange}
        onPausedChange={setPaused}
        paused={paused}
      />
      <LogList
        feed={feed}
        onPinnedChange={setPinned}
        onSelect={setSelected}
        pinned={pinned}
        selectedId={selected?.id ?? null}
      />
      <LogDetailSheet entry={selected} onClose={() => setSelected(null)} />
    </div>
  );
}
