import { IconPlus, IconStar, IconTrash } from "@tabler/icons-react";
import type { components } from "@tessera/api/schema";
import { Badge } from "@tessera/ui/components/badge";
import { Button } from "@tessera/ui/components/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@tessera/ui/components/table";
import { useState } from "react";

import {
  useSetDefaultStorageBackend,
  useStorageBackends,
} from "@/hooks/use-admin";
import { useSession } from "@/hooks/use-auth";
import { formatBytes } from "@/lib/format";
import { hasPermission, PERMISSIONS } from "@/lib/permissions";
import * as m from "@/paraglide/messages";

import { DeleteStorageBackendDialog } from "./delete-storage-backend-dialog";
import { SettingsCard } from "./settings-page";
import { StorageBackendEditorDialog } from "./storage-backend-editor-dialog";

type Backend = components["schemas"]["BackendResponse"];

/**
 * 檔案存哪裡不是一批 `settings::keys()`，是 `storage_backends` 表裡的列，走
 * `/admin/storage/backends` 系列端點，不經過 `PUT /admin/settings`。所以這張卡
 * 自己管自己的 query/mutation，不共用 `useSettingsForm`。
 *
 * `url` 和 `options` 是 sealed：`GET` 從不回傳它們，這張表只畫得出
 * `BackendResponse` 有回的欄位（容量、用量、四個布林），新增/覆蓋要填 `url` 得開
 * 對話框重新輸入。
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
      description={m.settings_storage_backends_card_description()}
      title={m.settings_storage_backends_card_title()}
    >
      {canWrite && (
        <Button
          className="w-fit"
          onClick={() => openEditor(null)}
          type="button"
        >
          <IconPlus />
          {m.settings_storage_backends_add()}
        </Button>
      )}

      {backends.isPending && (
        <p className="text-muted-foreground text-sm">{m.common_loading()}</p>
      )}
      {backends.isError && (
        <p className="text-destructive text-sm">
          {m.settings_storage_backends_load_error()}
        </p>
      )}
      {backends.data && (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>{m.settings_storage_backends_col_name()}</TableHead>
              <TableHead>{m.settings_storage_backends_col_status()}</TableHead>
              <TableHead className="text-right">
                {m.settings_storage_backends_col_used()}
              </TableHead>
              <TableHead className="text-right">
                {m.settings_storage_backends_col_capacity()}
              </TableHead>
              {canWrite && <TableHead className="w-24" />}
            </TableRow>
          </TableHeader>
          <TableBody>
            {backends.data.map((backend) => (
              <TableRow key={backend.name}>
                <TableCell className="font-medium">
                  <span className="flex items-center gap-2">
                    {backend.name}
                    {backend.is_default && (
                      <Badge variant="outline">
                        {m.settings_storage_backends_default_badge()}
                      </Badge>
                    )}
                  </span>
                </TableCell>
                <TableCell>
                  <span className="flex flex-wrap gap-1">
                    {!backend.reachable && (
                      <Badge variant="destructive">
                        {m.settings_storage_backends_unreachable_badge()}
                      </Badge>
                    )}
                    {backend.read_only && (
                      <Badge variant="secondary">
                        {m.settings_storage_backends_read_only_badge()}
                      </Badge>
                    )}
                    {backend.direct_access && (
                      <Badge variant="secondary">
                        {m.settings_storage_backends_direct_access_badge()}
                      </Badge>
                    )}
                  </span>
                </TableCell>
                <TableCell className="text-right tabular-nums">
                  {formatBytes(backend.used_bytes)}
                </TableCell>
                <TableCell className="text-right tabular-nums">
                  {backend.capacity_bytes === null ||
                  backend.capacity_bytes === undefined
                    ? m.settings_storage_backends_capacity_unlimited()
                    : formatBytes(backend.capacity_bytes)}
                </TableCell>
                {canWrite && (
                  <TableCell>
                    <span className="flex justify-end gap-1">
                      {!backend.is_default && (
                        <Button
                          aria-label={m.settings_storage_backends_set_default_aria(
                            { name: backend.name }
                          )}
                          disabled={setDefault.isPending}
                          onClick={() => setDefault.mutate(backend.name)}
                          size="icon"
                          type="button"
                          variant="ghost"
                        >
                          <IconStar />
                        </Button>
                      )}
                      <Button
                        aria-label={m.settings_storage_backends_delete_aria({
                          name: backend.name,
                        })}
                        disabled={backend.is_default}
                        onClick={() => openDelete(backend)}
                        size="icon"
                        type="button"
                        variant="ghost"
                      >
                        <IconTrash />
                      </Button>
                    </span>
                  </TableCell>
                )}
              </TableRow>
            ))}
          </TableBody>
        </Table>
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
