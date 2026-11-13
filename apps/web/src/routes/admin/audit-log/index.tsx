import { useQuery } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useSession } from "@tessera/auth/hooks";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import { useState } from "react";
import { z } from "zod";

import { useAuditEvents } from "@/hooks/admin/use-audit-events";
import type { AuditFilters } from "@/hooks/admin/use-audit-events";
import { readSetting, settingsQueryOptions } from "@/hooks/admin/use-settings";
import {
  AUDIT_CATEGORIES,
  AUDIT_OUTCOMES,
  CATEGORY_ACTIONS,
} from "@/lib/admin/audit-events";
import type { AuditEvent } from "@/lib/admin/audit-events";
import { hasPermission, PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

import { AuditDetailSheet } from "./-components/audit-detail-sheet";
import { AuditList } from "./-components/audit-list";
import { AuditToolbar } from "./-components/audit-toolbar";
import type { AuditSearch } from "./-components/audit-toolbar";

/** The setting that says how many days an entry is kept. */
const AUDIT_RETENTION_KEY = "audit.retention_days";

/** A value that does not parse drops that one filter, not the page. */
function lenient<T extends z.ZodType<string>>(schema: T) {
  // `undefined` is the value to fall back to, not a missing argument.
  // oxlint-disable-next-line promise/prefer-await-to-then, unicorn/no-useless-undefined -- zod's catch, not a Promise's
  return schema.optional().catch(undefined);
}

/**
 * The filters live in the URL, so a view can be sent to someone, and the user
 * and share pages can link straight to what was done by or to one of theirs.
 */
const auditSearchSchema = z.object({
  actor: lenient(z.string()),
  category: lenient(z.enum(AUDIT_CATEGORIES)),
  from: lenient(z.iso.datetime()),
  outcome: lenient(z.enum(AUDIT_OUTCOMES)),
  target_id: lenient(z.string()),
  target_type: lenient(z.string()),
  to: lenient(z.iso.datetime()),
});

export const Route = createFileRoute("/admin/audit-log/")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.auditRead
    ),
  component: AuditLogComponent,
  staticData: { title: m.audit_page_title },
  validateSearch: auditSearchSchema,
});

function AuditLogComponent() {
  const search = Route.useSearch();
  const navigate = useNavigate({ from: Route.fullPath });
  const [selected, setSelected] = useState<AuditEvent | null>(null);

  const filters: AuditFilters = {
    action: search.category ? CATEGORY_ACTIONS[search.category] : undefined,
    actor: search.actor,
    from: search.from,
    outcome: search.outcome,
    target_id: search.target_id,
    target_type: search.target_type,
    to: search.to,
  };
  const events = useAuditEvents(filters);

  const handleSearchChange = (next: Partial<AuditSearch>) => {
    navigate({
      replace: true,
      search: (previous) => ({ ...previous, ...next }),
    });
  };

  return (
    <Card className="ring-foreground/10">
      <CardHeader>
        <CardTitle>{m.audit_card_title()}</CardTitle>
        <CardDescription>
          <AuditCardDescription />
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <AuditToolbar
          filters={filters}
          onSearchChange={handleSearchChange}
          search={search}
        />
        <AuditList events={events} onSelect={setSelected} />
      </CardContent>
      <AuditDetailSheet event={selected} onClose={() => setSelected(null)} />
    </Card>
  );
}

/**
 * The card's one-line summary. The retention period is a setting, so it is read
 * rather than written into the copy; an account that may read the log but not
 * the settings gets the sentence without it instead of a number it cannot know.
 */
function AuditCardDescription() {
  const { data: account } = useSession();
  const canReadSettings = hasPermission(account, PERMISSIONS.settingsRead);
  const { data: settings } = useQuery({
    ...settingsQueryOptions,
    enabled: canReadSettings,
  });

  const days = readSetting(settings, AUDIT_RETENTION_KEY);
  if (typeof days !== "number") {
    return m.audit_card_description_plain();
  }
  return m.audit_card_description({ days });
}
