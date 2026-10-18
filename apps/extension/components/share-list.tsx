import { useQueryClient } from "@tanstack/react-query";
import { Button } from "@tessera/ui/components/button";
import { toast } from "@tessera/ui/components/toast";
import type { ShareResponse } from "@tessera/upload";
import { Copy, ExternalLink, Lock, LockKeyhole, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";

import { i18n } from "#i18n";
import { browser } from "#imports";
import { instanceKey, useMyShares } from "@/hooks/use-instance";
import type { Connection } from "@/hooks/use-instance";
import { useStorageItem } from "@/hooks/use-storage";
import { uploaderFor } from "@/lib/instance";
import { forgetShare, sharesItem } from "@/lib/storage";
import type { KeptShare } from "@/lib/storage";

/** A share as this list shows it, from the server, this browser, or both. */
interface Row {
  code: string;
  name: string;
  /** `null` for a sealed share whose key never reached this browser. */
  link: string | null;
  encrypted: boolean;
  hasPassword: boolean;
  createdAt: number;
  expiresAt: string | null;
  managementToken?: string;
}

/**
 * Signed in, the account's shares from the server, with the links and names
 * this browser kept for the sealed ones (the server has neither). Anonymous,
 * only what this browser kept: the management token in it is the only thing
 * that can delete such a share.
 */
export function ShareList({ connection }: { connection: Connection }) {
  const kept = useStorageItem(sharesItem);
  const mine = useMyShares(connection);
  const { server, credential } = connection;

  if (!server || kept === undefined || (credential && mine.isPending)) {
    return <ListMessage>{i18n.t("common.loading")}</ListMessage>;
  }
  if (credential && mine.isError) {
    return <ListMessage>{mine.error.message}</ListMessage>;
  }

  const keptHere = kept.filter((share) => share.server === server);
  const rows = credential
    ? (mine.data ?? []).map((share) => fromServer(server, share, keptHere))
    : keptHere.map(fromKept);

  if (rows.length === 0) {
    return <ListMessage>{i18n.t("shares.empty")}</ListMessage>;
  }
  return (
    <ul className="flex flex-col gap-1.5">
      {rows.map((row) => (
        <ShareRow connection={connection} key={row.code} row={row} />
      ))}
    </ul>
  );
}

function fromServer(
  server: string,
  share: ShareResponse,
  kept: KeptShare[]
): Row {
  const local = kept.find((entry) => entry.code === share.code);
  const encrypted = share.encryption !== null && share.encryption !== undefined;
  // Without the key after `#`, a share sealed with one opens for nobody.
  const keyMissing = share.encryption === "link" && !local;
  return {
    code: share.code,
    createdAt: Date.parse(share.created_at),
    encrypted,
    expiresAt: share.expires_at ?? null,
    hasPassword: share.has_password,
    link: keyMissing
      ? null
      : (local?.link ?? new URL(share.path, server).toString()),
    name:
      local?.name ??
      share.title ??
      share.files[0]?.filename ??
      (encrypted ? i18n.t("shares.sealed") : share.code),
  };
}

function fromKept(share: KeptShare): Row {
  return {
    code: share.code,
    createdAt: share.createdAt,
    encrypted: share.encrypted,
    expiresAt: null,
    hasPassword: false,
    link: share.link,
    managementToken: share.managementToken,
    name: share.name,
  };
}

/** How long the delete button waits for its second click. */
const CONFIRM_MS = 5000;

function ShareRow({ connection, row }: { connection: Connection; row: Row }) {
  const queryClient = useQueryClient();
  const [confirming, setConfirming] = useState(false);
  const [deleting, setDeleting] = useState(false);

  useEffect(() => {
    if (!confirming) {
      return;
    }
    const timer = setTimeout(() => setConfirming(false), CONFIRM_MS);
    return () => clearTimeout(timer);
  }, [confirming]);

  const handleCopy = async () => {
    if (!row.link) {
      return;
    }
    await navigator.clipboard.writeText(row.link);
    toast.add({ title: i18n.t("common.copied"), type: "success" });
  };

  const handleOpen = async () => {
    if (row.link) {
      await browser.tabs.create({ url: row.link });
    }
  };

  const handleDelete = async () => {
    if (!confirming) {
      setConfirming(true);
      return;
    }
    const { server, credential } = connection;
    if (!server) {
      return;
    }
    setDeleting(true);
    try {
      await uploaderFor(server, credential).deleteShare(
        row.code,
        credential ? undefined : row.managementToken
      );
      await forgetShare(server, row.code);
      await queryClient.invalidateQueries({
        queryKey: [...instanceKey(connection), "shares"],
      });
      toast.add({ title: i18n.t("shares.deleted"), type: "success" });
    } catch (error) {
      toast.add({
        description: error instanceof Error ? error.message : String(error),
        title: i18n.t("shares.deleteFailed"),
        type: "error",
      });
    } finally {
      setDeleting(false);
      setConfirming(false);
    }
  };

  // Anonymous and without its management token, nothing can delete it.
  const deletable = Boolean(connection.credential || row.managementToken);

  return (
    <li className="flex items-center gap-2 rounded-lg border bg-card px-3 py-2">
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="flex items-center gap-1 text-sm font-medium">
          {row.encrypted && (
            <LockKeyhole
              aria-label={i18n.t("shares.encrypted")}
              className="size-3.5 shrink-0 text-green-500"
            />
          )}
          {row.hasPassword && !row.encrypted && (
            <Lock
              aria-label={i18n.t("shares.password")}
              className="size-3.5 shrink-0 text-muted-foreground"
            />
          )}
          <span className="truncate" title={row.name}>
            {row.name}
          </span>
        </span>
        <span className="truncate text-xs text-muted-foreground">
          {row.link === null
            ? i18n.t("shares.keyMissing")
            : describeTimes(row.createdAt, row.expiresAt)}
        </span>
      </div>
      <Button
        aria-label={i18n.t("common.copyLink")}
        disabled={row.link === null}
        onClick={handleCopy}
        size="icon-sm"
        title={i18n.t("common.copyLink")}
        variant="ghost"
      >
        <Copy />
      </Button>
      <Button
        aria-label={i18n.t("common.open")}
        disabled={row.link === null}
        onClick={handleOpen}
        size="icon-sm"
        title={i18n.t("common.open")}
        variant="ghost"
      >
        <ExternalLink />
      </Button>
      {deletable &&
        (confirming ? (
          <Button
            disabled={deleting}
            onClick={handleDelete}
            size="xs"
            variant="destructive"
          >
            {i18n.t("shares.confirmDelete")}
          </Button>
        ) : (
          <Button
            aria-label={i18n.t("shares.delete")}
            onClick={handleDelete}
            size="icon-sm"
            title={i18n.t("shares.delete")}
            variant="ghost"
          >
            <Trash2 />
          </Button>
        ))}
    </li>
  );
}

function describeTimes(createdAt: number, expiresAt: string | null): string {
  const created = new Date(createdAt).toLocaleString(
    browser.i18n.getUILanguage(),
    { dateStyle: "medium", timeStyle: "short" }
  );
  if (!expiresAt) {
    return created;
  }
  const expires = new Date(expiresAt).toLocaleString(
    browser.i18n.getUILanguage(),
    { dateStyle: "medium", timeStyle: "short" }
  );
  return i18n.t("shares.expires", [created, expires]);
}

function ListMessage({ children }: { children: string }) {
  return (
    <p className="px-1 py-6 text-center text-sm text-muted-foreground">
      {children}
    </p>
  );
}
