import { Button } from "@tessera/ui/components/button";
import {
  ResponsiveDialog,
  ResponsiveDialogContent,
  ResponsiveDialogDescription,
  ResponsiveDialogFooter,
  ResponsiveDialogHeader,
  ResponsiveDialogTitle,
} from "@tessera/ui/components/responsive-dialog";
import { toast } from "@tessera/ui/components/toast";
import { Check, Copy, QrCode, Share2, ShieldAlert, Trash2 } from "lucide-react";
import { useState } from "react";
import type { ReactNode } from "react";

import { AnimatedQRCode } from "@/components/animated-qr-code";
import { useManagedShare } from "@/hooks/use-managed-share";
import { useShare } from "@/hooks/use-share";
import { apiErrorMessage } from "@/lib/api-error";
import { deleteShare, minutesUntil } from "@/lib/upload/share-api";
import * as m from "@/paraglide/messages";
import { useShareStore } from "@/stores/share";

import { ShareSettingsPanel } from "./share-settings-panel";
import type { ShareSettingsSnapshot } from "./share-settings-panel";

/** The share to manage. `initial` is the last settings the caller knows of. */
export interface ManagedShare {
  code: string;
  title: string;
  shortUrl: string;
  /** Only for anonymous uploads; a signed-in user's own shares use the session. */
  managementToken?: string;
  initial: ShareSettingsSnapshot;
  /**
   * Link-encrypted, and this browser does not have the key: `shortUrl` is
   * the bare link, which cannot open the share.
   */
  keyMissing?: boolean;
}

/**
 * Coming back after creation to change a share's settings or delete it.
 *
 * Shared by "my shares" and the home page's "recent uploads from this browser".
 * The first already holds what the server just sent and uses it. The second is
 * a snapshot stored in the browser that may have expired or been deleted long
 * ago, so `verify` asks the server once on open: what it returns wins, and a
 * 404/410 says the share is gone, leaving only removal from the list.
 */
export function ManageShareDialog({
  share,
  open,
  onOpenChange,
  verify = false,
  onSettingsChange,
  onDeleted,
  onForget,
}: {
  share: ManagedShare | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  verify?: boolean;
  onSettingsChange?: (code: string, snapshot: ShareSettingsSnapshot) => void;
  onDeleted?: (code: string) => void;
  /** When given, "remove from list" appears; it only touches this browser's record. */
  onForget?: (code: string) => void;
}) {
  return (
    <ResponsiveDialog onOpenChange={onOpenChange} open={open}>
      {/* A grid's implicit column defaults to auto, so a long title or URL
          stretches it past the dialog and overflows; an explicit
          minmax(0,1fr) lets truncate work. */}
      <ResponsiveDialogContent className="grid-cols-[minmax(0,1fr)] sm:max-w-md">
        {share &&
          (verify ? (
            <VerifiedShareContent
              key={share.code}
              onClose={() => onOpenChange(false)}
              onDeleted={onDeleted}
              onForget={onForget}
              onSettingsChange={onSettingsChange}
              share={share}
            />
          ) : (
            <ManageShareContent
              key={share.code}
              onClose={() => onOpenChange(false)}
              onDeleted={onDeleted}
              onForget={onForget}
              onSettingsChange={onSettingsChange}
              share={share}
            />
          ))}
      </ResponsiveDialogContent>
    </ResponsiveDialog>
  );
}

interface ContentProps {
  share: ManagedShare;
  onClose: () => void;
  onSettingsChange?: (code: string, snapshot: ShareSettingsSnapshot) => void;
  onDeleted?: (code: string) => void;
  onForget?: (code: string) => void;
}

/**
 * Confirms with the server that the share still exists before mounting the
 * form. The form reads its initial values once, so mounting first and swapping
 * later would flash the switches in the wrong state.
 *
 * For a password-protected share without the password, the server only says
 * it has one and gives no expiry or use count; the snapshot is used then.
 */
function VerifiedShareContent(props: ContentProps) {
  const live = useShare(props.share.code);

  // Wait for the copy fetched after this open. The cache may hold the value
  // from the last open, and the form reads only once on mount, so a stale copy
  // would show a just-changed expiry as it was before.
  if (!(live.isFetchedAfterMount || live.isError)) {
    return (
      <>
        <ShareHeader share={props.share} />
        <p className="py-6 text-center text-muted-foreground text-sm">
          {m.common_loading()}
        </p>
      </>
    );
  }

  const status = (live.error as { status?: number } | null)?.status;
  if (status === 404 || status === 410) {
    return <GoneShareContent {...props} />;
  }

  const { data } = live;
  const initial: ShareSettingsSnapshot =
    data?.unlocked === true
      ? {
          burnAfterReading: data.uses_remaining === 1,
          expiresInMinutes: minutesUntil(data.expires_at),
          hasPassword: data.has_password,
          passwordIsKey: data.encryption === "password",
        }
      : {
          ...props.share.initial,
          hasPassword: data?.has_password ?? true,
          passwordIsKey:
            data?.encryption === "password" ||
            props.share.initial.passwordIsKey,
        };

  return <ManageShareContent {...props} share={{ ...props.share, initial }} />;
}

function ManageShareContent({
  share,
  onClose,
  onSettingsChange,
  onDeleted,
  onForget,
}: ContentProps) {
  const [confirming, setConfirming] = useState(false);
  const [confirmingForget, setConfirmingForget] = useState(false);
  const [deleting, setDeleting] = useState(false);

  const handleDelete = async () => {
    setDeleting(true);
    try {
      await deleteShare(share.code, share.managementToken);
      onDeleted?.(share.code);
      onClose();
    } catch (error) {
      toast.add({
        description: apiErrorMessage(error, m.common_try_again()),
        title: m.upload_result_delete_failed(),
        type: "error",
      });
    } finally {
      setDeleting(false);
    }
  };

  const footerActions: ReactNode = (() => {
    if (confirming) {
      return (
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-sm">{m.manage_share_delete_confirm()}</span>
          <Button
            disabled={deleting}
            onClick={handleDelete}
            size="sm"
            type="button"
            variant="destructive"
          >
            {m.upload_result_delete()}
          </Button>
          <Button
            disabled={deleting}
            onClick={() => setConfirming(false)}
            size="sm"
            type="button"
            variant="ghost"
          >
            {m.common_cancel()}
          </Button>
        </div>
      );
    }

    if (confirmingForget) {
      return (
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-sm">{m.manage_share_forget_hint()}</span>
          <Button
            onClick={() => {
              onForget?.(share.code);
              onClose();
            }}
            size="sm"
            type="button"
            variant="outline"
          >
            {m.manage_share_forget()}
          </Button>
          <Button
            onClick={() => setConfirmingForget(false)}
            size="sm"
            type="button"
            variant="ghost"
          >
            {m.common_cancel()}
          </Button>
        </div>
      );
    }

    return (
      <div className="flex flex-wrap items-center gap-1">
        <Button
          className="text-destructive hover:text-destructive"
          onClick={() => setConfirming(true)}
          size="sm"
          type="button"
          variant="ghost"
        >
          <Trash2 />
          {m.manage_share_delete()}
        </Button>
        {onForget && (
          <Button
            onClick={() => setConfirmingForget(true)}
            size="sm"
            type="button"
            variant="ghost"
          >
            {m.manage_share_forget()}
          </Button>
        )}
      </div>
    );
  })();

  return (
    <>
      <ShareHeader share={share} />

      <div className="rounded-lg border p-3">
        <ShareSettingsPanel
          code={share.code}
          initial={share.initial}
          managementToken={share.managementToken}
          onChange={(snapshot) => onSettingsChange?.(share.code, snapshot)}
        />
      </div>

      <ResponsiveDialogFooter className="sm:justify-between">
        {footerActions}
        <Button onClick={onClose} type="button">
          {m.manage_share_done()}
        </Button>
      </ResponsiveDialogFooter>
    </>
  );
}

/**
 * The share expired, ran out of uses or was deleted: nothing to change, only removal from
 * the list.
 */
function GoneShareContent({ share, onClose, onForget }: ContentProps) {
  const [confirmingForget, setConfirmingForget] = useState(false);
  // The public description says only "gone". Its manager may ask why, and a
  // share switched off for malware should not read as merely expired.
  const managed = useManagedShare(share.code, share.managementToken);
  const blocked =
    managed.data?.disabled && managed.data.scan_state === "infected";

  return (
    <>
      <ShareHeader share={share} />
      {blocked ? (
        <p className="flex items-start gap-2 text-destructive text-sm">
          <ShieldAlert aria-hidden className="mt-0.5 size-4 shrink-0" />
          {m.manage_share_blocked({
            threat: managed.data?.scan_detail ?? "?",
          })}
        </p>
      ) : (
        <p className="text-muted-foreground text-sm">{m.manage_share_gone()}</p>
      )}
      <ResponsiveDialogFooter>
        {onForget &&
          (confirmingForget ? (
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-sm">{m.manage_share_forget_hint()}</span>
              <Button
                onClick={() => {
                  onForget(share.code);
                  onClose();
                }}
                type="button"
                variant="outline"
              >
                {m.manage_share_forget()}
              </Button>
              <Button
                onClick={() => setConfirmingForget(false)}
                type="button"
                variant="ghost"
              >
                {m.common_cancel()}
              </Button>
            </div>
          ) : (
            <Button
              onClick={() => setConfirmingForget(true)}
              type="button"
              variant="outline"
            >
              {m.manage_share_forget()}
            </Button>
          ))}
        <Button onClick={onClose} type="button">
          {m.manage_share_done()}
        </Button>
      </ResponsiveDialogFooter>
    </>
  );
}

function ShareHeader({ share }: { share: ManagedShare }) {
  const [copied, setCopied] = useState(false);
  const [showQr, setShowQr] = useState(false);
  const canShare = useShareStore((state) => state.canShare);
  const shareNative = useShareStore((state) => state.share);

  const handleCopy = async () => {
    await navigator.clipboard.writeText(share.shortUrl);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  const handleShare = async () => {
    const outcome = await shareNative({
      title: share.title,
      url: share.shortUrl,
    });
    if (outcome.status === "failed") {
      toast.add({
        description: apiErrorMessage(outcome.error, m.common_try_again()),
        title: m.upload_result_share_failed(),
        type: "error",
      });
    }
  };

  return (
    <ResponsiveDialogHeader>
      <ResponsiveDialogTitle className="truncate pr-6">
        {share.title}
      </ResponsiveDialogTitle>
      <ResponsiveDialogDescription className="flex items-center gap-1">
        <span className="min-w-0 truncate font-mono text-xs">
          {share.shortUrl}
        </span>
        <Button
          aria-label={m.manage_share_copy_link()}
          className="shrink-0"
          onClick={handleCopy}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          {copied ? <Check /> : <Copy />}
        </Button>
        {canShare && (
          <Button
            aria-label={m.upload_result_share()}
            className="shrink-0"
            onClick={handleShare}
            size="icon-sm"
            type="button"
            variant="ghost"
          >
            <Share2 />
          </Button>
        )}
        <Button
          aria-label={m.manage_share_qr()}
          aria-pressed={showQr}
          className="shrink-0"
          onClick={() => setShowQr((prev) => !prev)}
          size="icon-sm"
          type="button"
          variant={showQr ? "secondary" : "ghost"}
        >
          <QrCode />
        </Button>
      </ResponsiveDialogDescription>
      {share.keyMissing && (
        <p className="text-destructive text-xs">
          {m.manage_share_key_missing()}
        </p>
      )}
      {showQr && (
        // Dark code on white, in dark mode too: some scanners cannot read an inverted QR
        // code.
        <div className="mx-auto mt-2 w-fit overflow-hidden rounded-xl bg-white ring-1 ring-foreground/10">
          <AnimatedQRCode size={176} value={share.shortUrl} />
        </div>
      )}
    </ResponsiveDialogHeader>
  );
}
