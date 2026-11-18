import { IconDownload, IconX } from "@tabler/icons-react";
import { Badge } from "@tessera/ui/components/badge";
import { Button, buttonVariants } from "@tessera/ui/components/button";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@tessera/ui/components/select";
import { cn } from "@tessera/ui/lib/utils";

import { DateTimePicker } from "@/components/admin/date-time-picker";
import { auditExportUrl } from "@/hooks/admin/use-audit-events";
import type { AuditFilters } from "@/hooks/admin/use-audit-events";
import {
  AUDIT_CATEGORIES,
  AUDIT_OUTCOMES,
  CATEGORY_LABELS,
  OUTCOME_LABELS,
} from "@/lib/admin/audit-events";
import type { AuditCategory, AuditOutcome } from "@/lib/admin/audit-events";
import * as m from "@/paraglide/messages";

const ANY = "all";

/** The page's search params, as the toolbar reads and writes them. */
export interface AuditSearch {
  actor?: string;
  category?: AuditCategory;
  from?: string;
  outcome?: AuditOutcome;
  target_id?: string;
  target_type?: string;
  to?: string;
}

interface AuditToolbarProps {
  search: AuditSearch;
  filters: AuditFilters;
  onSearchChange: (next: Partial<AuditSearch>) => void;
}

export function AuditToolbar({
  search,
  filters,
  onSearchChange,
}: AuditToolbarProps) {
  const hasRange = Boolean(search.from || search.to);

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <Select
          onValueChange={(value) =>
            onSearchChange({
              category: value === ANY ? undefined : (value as AuditCategory),
            })
          }
          value={search.category ?? ANY}
        >
          <SelectTrigger aria-label={m.audit_category_label()} className="w-36">
            <SelectValue>
              {(value: string) =>
                value === ANY
                  ? m.audit_category_all()
                  : CATEGORY_LABELS[value as AuditCategory]()
              }
            </SelectValue>
          </SelectTrigger>
          <SelectContent alignItemWithTrigger={false}>
            <SelectGroup>
              <SelectItem value={ANY}>{m.audit_category_all()}</SelectItem>
              {AUDIT_CATEGORIES.map((category) => (
                <SelectItem key={category} value={category}>
                  {CATEGORY_LABELS[category]()}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>

        <Select
          onValueChange={(value) =>
            onSearchChange({
              outcome: value === ANY ? undefined : (value as AuditOutcome),
            })
          }
          value={search.outcome ?? ANY}
        >
          <SelectTrigger aria-label={m.audit_outcome_label()} className="w-32">
            <SelectValue>
              {(value: string) =>
                value === ANY
                  ? m.audit_outcome_all()
                  : OUTCOME_LABELS[value as AuditOutcome]()
              }
            </SelectValue>
          </SelectTrigger>
          <SelectContent alignItemWithTrigger={false}>
            <SelectGroup>
              <SelectItem value={ANY}>{m.audit_outcome_all()}</SelectItem>
              {AUDIT_OUTCOMES.map((outcome) => (
                <SelectItem key={outcome} value={outcome}>
                  {OUTCOME_LABELS[outcome]()}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>

        <div className="grid w-full grid-cols-[1fr_auto_1fr_auto] items-center gap-1 sm:flex sm:w-auto">
          <DateTimePicker
            className="w-full sm:w-auto"
            label={m.audit_from_label()}
            onChange={(from) => onSearchChange({ from: from ?? undefined })}
            value={search.from ?? null}
          />
          <span aria-hidden="true">–</span>
          <DateTimePicker
            className="w-full sm:w-auto"
            label={m.audit_to_label()}
            onChange={(to) => onSearchChange({ to: to ?? undefined })}
            value={search.to ?? null}
          />
          {hasRange ? (
            <Button
              aria-label={m.audit_clear_range()}
              onClick={() => onSearchChange({ from: undefined, to: undefined })}
              size="icon"
              type="button"
              variant="ghost"
            >
              <IconX />
            </Button>
          ) : (
            <span className="size-8 sm:hidden" />
          )}
        </div>

        {/* A plain link, so the browser streams the file to disk. */}
        <a
          className={cn(buttonVariants({ variant: "outline" }), "ml-auto")}
          download
          href={auditExportUrl(filters)}
        >
          <IconDownload />
          {m.audit_export()}
        </a>
      </div>

      {(search.actor || search.target_type) && (
        <div className="flex flex-wrap items-center gap-2">
          {search.actor && (
            <FilterChip
              label={m.audit_filter_actor({ id: search.actor })}
              onClear={() => onSearchChange({ actor: undefined })}
            />
          )}
          {search.target_type && (
            <FilterChip
              label={m.audit_filter_target({
                target: search.target_id
                  ? `${search.target_type} · ${search.target_id}`
                  : search.target_type,
              })}
              onClear={() =>
                onSearchChange({ target_id: undefined, target_type: undefined })
              }
            />
          )}
        </div>
      )}
    </div>
  );
}

/** A filter set from elsewhere (a user or share page), with a way to drop it. */
function FilterChip({
  label,
  onClear,
}: {
  label: string;
  onClear: () => void;
}) {
  return (
    <Badge className="gap-1 pr-0.5 font-mono" variant="secondary">
      <span className="max-w-72 truncate">{label}</span>
      <Button
        aria-label={m.audit_filter_clear()}
        className="size-5"
        onClick={onClear}
        size="icon"
        type="button"
        variant="ghost"
      >
        <IconX className="size-3" />
      </Button>
    </Badge>
  );
}
