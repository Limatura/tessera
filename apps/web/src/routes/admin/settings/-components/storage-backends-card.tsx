import {
  IconDotsVertical,
  IconPencil,
  IconPlus,
  IconStar,
  IconTrash,
} from "@tabler/icons-react";
import type { components } from "@tessera/api/schema";
import { useSession } from "@tessera/auth/hooks";
import { Badge } from "@tessera/ui/components/badge";
import { Button } from "@tessera/ui/components/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@tessera/ui/components/dropdown-menu";
import { formatBytes } from "@tessera/ui/lib/format";
import { cn } from "@tessera/ui/lib/utils";
import { motion } from "motion/react";
import { useState } from "react";

import { ListError, ListSkeleton } from "@/components/admin/list-state";
import {
  CRITICAL_PERCENT,
  SegmentedUsageBar,
  segmentOpacity,
  usagePercent,
  WARN_PERCENT,
} from "@/components/admin/usage-bar";
import {
  useSetDefaultStorageBackend,
  useStorageBackends,
} from "@/hooks/admin/use-storage-backends";
import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import { hasPermission, PERMISSIONS } from "@/lib/admin/permissions";
import type { StorageProvider } from "@/lib/admin/storage-provider";
import { storageProviderOf } from "@/lib/admin/storage-provider";
import * as m from "@/paraglide/messages";

import { DeleteStorageBackendDialog } from "./delete-storage-backend-dialog";
import { SettingsCard } from "./settings-page";
import { StorageBackendEditorDialog } from "./storage-backend-editor-dialog";
import { WaveEdge } from "./wave-edge";

type Backend = components["schemas"]["BackendResponse"];

/**
 * Where files are stored is not a set of `settings::keys()` but rows in the
 * `storage_backends` table, behind the `/admin/storage/backends` endpoints
 * instead of `PUT /admin/settings`. So this card owns its queries and mutations
 * and does not share `useSettingsForm`.
 *
 * One card per backend: how full it is matters more than either number alone,
 * and which one takes new uploads matters most, so both get room a table row
 * would not give them. The server sends the location without credentials and
 * the access key ID masked, enough to tell two buckets apart.
 */
export function StorageBackendsCard() {
  const backends = useStorageBackends();
  const { data: account } = useSession();
  const setDefault = useSetDefaultStorageBackend();

  const [editing, setEditing] = useState<Backend | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [deleting, setDeleting] = useState<Backend | null>(null);
  const [deleteOpen, setDeleteOpen] = useState(false);

  const canWrite = hasPermission(account, PERMISSIONS.settingsWrite);

  const openEditor = (backend: Backend | null) => {
    setEditing(backend);
    setEditorOpen(true);
  };

  const openDelete = (backend: Backend) => {
    setDeleting(backend);
    setDeleteOpen(true);
  };

  return (
    <SettingsCard
      action={
        canWrite && (
          <Button
            onClick={() => openEditor(null)}
            size="sm"
            type="button"
            variant="outline"
          >
            <IconPlus />
            {m.settings_storage_backends_add()}
          </Button>
        )
      }
      description={m.settings_storage_backends_card_description()}
      title={m.settings_storage_backends_card_title()}
    >
      {backends.isPending && <ListSkeleton rows={2} />}
      {backends.isError && (
        <ListError
          isRetrying={backends.isFetching}
          message={m.settings_storage_backends_load_error()}
          onRetry={() => backends.refetch()}
        />
      )}
      {backends.data && (
        <StorageSplit backends={sortDefaultFirst(backends.data)} />
      )}
      {backends.data && (
        <ul className="grid gap-2 md:grid-cols-2">
          {sortDefaultFirst(backends.data).map((backend) => (
            <BackendCard
              backend={backend}
              canWrite={canWrite}
              isSettingDefault={setDefault.isPending}
              key={backend.name}
              onDelete={() => openDelete(backend)}
              onEdit={() => openEditor(backend)}
              onSetDefault={() => setDefault.mutate(backend.name)}
            />
          ))}
        </ul>
      )}

      {canWrite && (
        <StorageBackendEditorDialog
          backend={editing}
          onOpenChange={setEditorOpen}
          open={editorOpen}
        />
      )}
      {canWrite && (
        <DeleteStorageBackendDialog
          backend={deleting}
          onOpenChange={setDeleteOpen}
          open={deleteOpen}
        />
      )}
    </SettingsCard>
  );
}

/** The default backend leads, since it is where new uploads go; the rest by name. */
function sortDefaultFirst(list: Backend[]): Backend[] {
  return list.toSorted(
    (a, b) =>
      Number(b.is_default) - Number(a.is_default) ||
      a.name.localeCompare(b.name)
  );
}

/**
 * One card per backend, and the card is the gauge: its background fills from
 * the left, in the provider's color, as far as the backend is full, so how
 * full it is needs no bar of its own. Name, status and location on the left,
 * the figure in large type on the right.
 */
function BackendCard({
  backend,
  canWrite,
  isSettingDefault,
  onSetDefault,
  onEdit,
  onDelete,
}: {
  backend: Backend;
  canWrite: boolean;
  isSettingDefault: boolean;
  onSetDefault: () => void;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const provider = storageProviderOf(backend);

  return (
    <li
      className={cn(
        "relative isolate flex items-center gap-3 rounded-lg border px-3 py-3",
        !backend.reachable && "border-destructive/50 bg-destructive/5"
      )}
    >
      <FillLayer backend={backend} color={provider.color} />
      {/* Shared layout id: switching the default slides the ring across. */}
      {backend.is_default && (
        <motion.span
          aria-hidden
          className="pointer-events-none absolute -inset-px rounded-lg ring-2 ring-primary"
          layoutId="default-storage-backend"
          transition={{ type: "spring", bounce: 0.2, duration: 0.4 }}
        />
      )}

      <ProviderTile provider={provider} />

      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
          <ReachableDot reachable={backend.reachable} />
          <span className="truncate font-medium text-sm">{backend.name}</span>
          {backend.is_default && (
            <Badge>{m.settings_storage_backends_default_badge()}</Badge>
          )}
          <BackendFlags backend={backend} />
        </div>
        <BackendLocation backend={backend} />
        <BackendHealth backend={backend} />
      </div>

      <BackendFigures backend={backend} />
      {canWrite && (
        <BackendActions
          backend={backend}
          isSettingDefault={isSettingDefault}
          onDelete={onDelete}
          onEdit={onEdit}
          onSetDefault={onSetDefault}
        />
      )}
    </li>
  );
}

/** `s3://bucket/prefix · AKIA…MPLE`, truncated, with the whole of it on hover. */
function BackendLocation({ backend }: { backend: Backend }) {
  const parts = [backend.url, backend.access_key].filter(Boolean).join(" · ");
  return (
    <span
      className="truncate font-mono text-muted-foreground text-xs"
      title={parts}
    >
      {parts}
    </span>
  );
}

/**
 * When the store was last called and, if that failed, why. Nothing before the
 * first call after start-up. The reason is the store's own wording, so it stays
 * untranslated; truncated, with the whole of it on hover.
 */
function BackendHealth({ backend }: { backend: Backend }) {
  const formatDateTime = useDateTimeFormatter();
  if (!backend.last_checked_at) {
    return null;
  }
  return (
    <>
      {backend.last_error && (
        <span
          className="truncate text-destructive text-xs"
          title={backend.last_error}
        >
          {backend.last_error}
        </span>
      )}
      <span className="truncate text-muted-foreground text-xs">
        {m.settings_storage_backends_last_checked({
          time: formatDateTime(backend.last_checked_at),
        })}
      </span>
    </>
  );
}

/** Used, in large type; capacity and percentage, or no limit, small beneath. */
function BackendFigures({ backend }: { backend: Backend }) {
  const capacity = backend.capacity_bytes ?? 0;
  return (
    <div className="flex shrink-0 flex-col items-end tabular-nums leading-tight">
      <span className="font-semibold text-base">
        {formatBytes(backend.used_bytes)}
      </span>
      <span className="text-muted-foreground text-xs">
        {capacity === 0
          ? m.settings_storage_backends_of_unlimited()
          : m.settings_storage_backends_of_capacity({
              capacity: formatBytes(capacity),
              percent: usagePercent(backend.used_bytes, capacity),
            })}
      </span>
    </div>
  );
}

/**
 * The card's background, filled as far as the backend is full: a faint wash
 * with a moving wave for its leading edge. In the provider's
 * color until it nears full, then amber and red at the thresholds `UsageBar`
 * uses. Nothing for a backend with no limit, where a level would mean nothing.
 *
 * Decorative only: `BackendFigures` says the same in words. Clipped by its own
 * layer so the default ring outside the card is not cut.
 */
function FillLayer({ backend, color }: { backend: Backend; color: string }) {
  const capacity = backend.capacity_bytes ?? 0;
  if (capacity === 0) {
    return null;
  }
  const percent = usagePercent(backend.used_bytes, capacity);
  const tone =
    (percent >= CRITICAL_PERCENT && "text-destructive") ||
    (percent >= WARN_PERCENT && "text-amber-500") ||
    color;

  return (
    <span
      aria-hidden
      className="pointer-events-none absolute inset-0 -z-10 overflow-hidden rounded-lg"
    >
      <span
        className={cn("absolute inset-y-0 left-0 transition-[width]", tone)}
        style={{ width: `${Math.min(percent, 100)}%` }}
      >
        <span className="absolute inset-0 bg-current opacity-10" />
        {percent > 0 && percent < 100 && <WaveEdge />}
      </span>
    </span>
  );
}

/** Green when the store opened, red when not; `BackendFlags` says the latter in words. */
function ReachableDot({ reachable }: { reachable: boolean }) {
  return (
    <>
      <span
        aria-hidden
        className={cn(
          "size-2 shrink-0 rounded-full",
          reachable ? "bg-emerald-500" : "bg-destructive"
        )}
      />
      {reachable && (
        <span className="sr-only">
          {m.settings_storage_backends_reachable()}
        </span>
      )}
    </>
  );
}

/** What else is true of it, as quiet words after the name. */
function BackendFlags({ backend }: { backend: Backend }) {
  const flags = [
    !backend.reachable && (
      <span className="text-destructive" key="unreachable">
        {m.settings_storage_backends_unreachable_badge()}
      </span>
    ),
    backend.read_only && (
      <span key="read-only">
        {m.settings_storage_backends_read_only_badge()}
      </span>
    ),
    backend.direct_access && (
      <span key="direct">
        {m.settings_storage_backends_direct_access_badge()}
      </span>
    ),
  ].filter(Boolean);

  return flags.length > 0 ? (
    <span className="flex flex-wrap items-center gap-x-2 text-muted-foreground text-xs">
      {flags}
    </span>
  ) : null;
}

function BackendActions({
  backend,
  isSettingDefault,
  onSetDefault,
  onEdit,
  onDelete,
}: {
  backend: Backend;
  isSettingDefault: boolean;
  onSetDefault: () => void;
  onEdit: () => void;
  onDelete: () => void;
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            aria-label={m.settings_storage_backends_actions_aria({
              name: backend.name,
            })}
            className="-mr-1.5 shrink-0 max-md:size-10"
            size="icon"
            type="button"
            variant="ghost"
          />
        }
      >
        <IconDotsVertical />
      </DropdownMenuTrigger>
      {/* One width for every card's menu, whatever it holds: by default the
          popup takes the trigger's width, too narrow for "Make default". */}
      <DropdownMenuContent align="end" className="w-44">
        {!backend.is_default && (
          <DropdownMenuItem disabled={isSettingDefault} onClick={onSetDefault}>
            <IconStar />
            {m.settings_storage_backends_set_default()}
          </DropdownMenuItem>
        )}
        <DropdownMenuItem onClick={onEdit}>
          <IconPencil />
          {m.settings_storage_backends_edit()}
        </DropdownMenuItem>
        {/* The default cannot be removed (the server refuses), so it gets no
            item at all rather than a disabled one that needs explaining. */}
        {!backend.is_default && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem onClick={onDelete} variant="destructive">
              <IconTrash />
              {m.settings_storage_backends_delete()}
            </DropdownMenuItem>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/**
 * Where the instance's content lives, split by backend, with a legend naming
 * each segment. Hidden until something is stored, and with a single backend,
 * where the split would be one full bar saying nothing.
 */
function StorageSplit({ backends }: { backends: Backend[] }) {
  const total = backends.reduce((sum, backend) => sum + backend.used_bytes, 0);
  if (total === 0 || backends.length < 2) {
    return null;
  }
  const segments = backends.map((backend) => ({
    name: backend.name,
    usedBytes: backend.used_bytes,
  }));

  return (
    <div className="mb-4 flex flex-col gap-2">
      <SegmentedUsageBar segments={segments} />
      <ul className="flex flex-wrap gap-x-4 gap-y-1 text-muted-foreground text-xs tabular-nums">
        {backends.map((backend, index) => (
          <li className="flex items-center gap-1.5" key={backend.name}>
            <span
              aria-hidden
              className="size-2 rounded-full bg-primary"
              style={{ opacity: segmentOpacity(index) }}
            />
            <span className="text-foreground">{backend.name}</span>
            {usagePercent(backend.used_bytes, total)}%
          </li>
        ))}
      </ul>
    </div>
  );
}

/** The provider's logo in its own color, on a tile tinted the same. */
function ProviderTile({ provider }: { provider: StorageProvider }) {
  const { icon: Icon, color } = provider;
  return (
    <span
      aria-hidden
      className={cn(
        "relative flex size-10 shrink-0 items-center justify-center rounded-lg",
        color
      )}
    >
      <span className="absolute inset-0 rounded-lg bg-current opacity-15" />
      <Icon className="relative size-6" stroke={1.75} />
    </span>
  );
}
