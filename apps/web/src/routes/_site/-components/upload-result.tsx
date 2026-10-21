import { IconExternalLink } from "@tabler/icons-react";
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
import type { UploadResultData } from "@tessera/upload";
import {
  Check,
  Copy,
  LockKeyhole,
  RotateCcw,
  Share2,
  Trash2,
} from "lucide-react";
import { motion } from "motion/react";
import { useEffect, useState } from "react";

import { AnimatedQRCode } from "@/components/animated-qr-code";
import { ScanStatus } from "@/components/scan/scan-status";
import { useManagedShare } from "@/hooks/use-managed-share";
import { apiErrorMessage } from "@/lib/api-error";
import { copyText } from "@/lib/copy-text";
import { uploader } from "@/lib/uploader";
import * as m from "@/paraglide/messages";
import { useRecentSharesStore } from "@/stores/recent-shares";
import { useShareStore } from "@/stores/share";
import { useShareKeysStore } from "@/stores/share-keys";

import { ShareSettingsPanel, expiresAtFrom } from "./share-settings-panel";
import type { ShareSettingsSnapshot } from "./share-settings-panel";
import { revealArrival } from "./use-submit-transition";
import type { Origin } from "./use-submit-transition";

/**
 * Background for blocks on the overlay. The overlay sits on --melt, not
 * background/card, so shades come from currentColor at low opacity: each
 * mode's --melt-foreground turns that into darkening or lightening by itself,
 * with no separate dark: variants.
 */
const PANEL = "rounded-lg border border-current/15 bg-current/5";

interface UploadResultProps {
  onReset: () => void;
  /** Where the circular reveal grows from, so the QR ripple can pick up its edge. */
  origin: Origin;
  result: UploadResultData;
  /**
   * Stays mounted while collapsing and only fades its content, so tearing down the
   * subtree does not stall the first frame
   */
  visible: boolean;
}

export function UploadResult({
  onReset,
  origin,
  result,
  visible,
}: UploadResultProps) {
  const [copied, setCopied] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const remember = useRecentSharesStore((state) => state.remember);
  const updateRecent = useRecentSharesStore((state) => state.update);
  const forgetRecent = useRecentSharesStore((state) => state.forget);
  // The verdict at creation, then the server's as it changes: a pending scan
  // turns clean, or infected, while this screen is still up.
  const live = useManagedShare(
    result.code,
    result.managementToken,
    result.scanState !== null
  );
  const scan =
    result.scanState === null
      ? null
      : (live.data ?? { scan_detail: null, scan_state: result.scanState });
  const canShare = useShareStore((state) => state.canShare);
  const share = useShareStore((state) => state.share);

  const rememberKey = useShareKeysStore((state) => state.remember);
  // An anonymous upload's management token exists only on this screen and is
  // lost when it closes, so it goes into this browser's "recent uploads" to
  // come back to from the home page. A signed-in user's shares have no token
  // and belong to the account.
  // A signed-in upload is not kept above, but its key must be: My shares lists
  // it from the server, which never had the key.
  useEffect(() => {
    const [, fragment] = result.shortUrl.split("#");
    if (!result.managementToken && fragment) {
      rememberKey(result.code, fragment);
    }
  }, [result, rememberKey]);

  useEffect(() => {
    if (!result.managementToken) {
      return;
    }
    remember({
      code: result.code,
      createdAt: new Date().toISOString(),
      expiresAt: expiresAtFrom(result.expiresInMinutes),
      fileName: result.fileName,
      hasPassword: result.hasPassword,
      managementToken: result.managementToken,
      passwordIsKey: result.passwordIsKey,
      shortUrl: result.shortUrl,
      usesRemaining: result.maxUses ?? null,
    });
  }, [result, remember]);

  const handleSettingsChange = (snapshot: ShareSettingsSnapshot) => {
    updateRecent(result.code, {
      expiresAt: expiresAtFrom(snapshot.expiresInMinutes),
      hasPassword: snapshot.hasPassword,
      usesRemaining: snapshot.burnAfterReading ? 1 : null,
    });
  };

  const handleCopy = async () => {
    if (!(await copyText(result.shortUrl))) {
      return;
    }
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  const handleShare = async () => {
    const outcome = await share({
      title: result.fileName,
      url: result.shortUrl,
    });
    if (outcome.status === "failed") {
      toast.add({
        description: apiErrorMessage(outcome.error, m.common_try_again()),
        title: m.upload_result_share_failed(),
        type: "error",
      });
    }
  };

  const handleDelete = async () => {
    setDeleting(true);
    try {
      await uploader.deleteShare(result.code, result.managementToken);
      forgetRecent(result.code);
      onReset();
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

  return (
    <motion.div
      animate={visible ? { opacity: 1, y: 0 } : { opacity: 0, y: 0 }}
      className="flex w-full max-w-sm flex-col items-center gap-6 text-center text-melt-foreground p-2"
      initial={{ opacity: 0, y: 12 }}
      transition={
        visible
          ? { duration: 0.4, ease: [0.16, 1, 0.3, 1] }
          : { duration: 0.15, ease: "linear" }
      }
    >
      {/*
        The QR code stays dark on white: some scanners cannot read an inverted
        one. In dark mode the white square stands as its own card, edged by the
        outer ring, instead of darkening with the theme.
      */}
      <div className="overflow-hidden rounded-xl bg-white ring-1 ring-current/15 dark:shadow-black/40 dark:shadow-lg mt-24">
        {/* The ripple starts where, and when, the reveal's edge reaches the code */}
        <AnimatedQRCode
          play={visible}
          wave={{
            arrival: (distance) => revealArrival(origin, distance),
            x: origin.x,
            y: origin.y,
          }}
          size={176}
          value={result.shortUrl}
        />
      </div>

      <div className="flex w-full min-w-0 flex-col gap-1">
        <p className="flex w-full min-w-0 items-center justify-center gap-1 font-medium">
          <span className="min-w-0 truncate pl-2">{result.fileName}</span>
          {result.encrypted && (
            <LockKeyhole
              aria-label={m.share_encrypted_badge()}
              className="size-4 shrink-0 text-green-500"
            />
          )}
        </p>
        {!scan && (
          <p className="text-current/70 text-xs">{m.upload_result_ready()}</p>
        )}
      </div>

      {scan && <ScanStatus className="w-full" share={scan} />}

      <div className={`flex w-full items-center gap-2 px-3 py-2 ${PANEL}`}>
        <span className="flex-1 truncate text-left font-mono text-xs">
          {result.shortUrl}
        </span>
        {canShare && (
          <Button
            aria-label={m.upload_result_share()}
            className="shrink-0 text-current hover:bg-current/10 hover:text-current"
            onClick={handleShare}
            size="icon-sm"
            type="button"
            variant="ghost"
          >
            <Share2 />
          </Button>
        )}
        <Button
          className="shrink-0 text-current hover:bg-current/10 hover:text-current"
          nativeButton={false}
          render={
            <a
              aria-label={m.upload_result_open()}
              href={result.shortUrl}
              rel="noopener"
              target="_blank"
            />
          }
          size="icon-sm"
          variant="ghost"
        >
          <IconExternalLink />
        </Button>
        <Button
          aria-label={m.share_copy()}
          className="shrink-0 text-current hover:bg-current/10 hover:text-current"
          onClick={handleCopy}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          {copied ? <Check /> : <Copy />}
        </Button>
      </div>

      <div className={`w-full p-3 text-left ${PANEL}`}>
        <ShareSettingsPanel
          code={result.code}
          initial={{
            burnAfterReading: result.maxUses === 1,
            expiresInMinutes: result.expiresInMinutes,
            hasPassword: result.hasPassword,
            passwordIsKey: result.passwordIsKey,
          }}
          managementToken={result.managementToken}
          onChange={handleSettingsChange}
        />
      </div>

      <div className="flex w-full items-center gap-2">
        <Button
          className="flex-1 border-current/25 text-current hover:bg-current/10 hover:text-current"
          onClick={onReset}
          type="button"
          variant="outline"
        >
          <RotateCcw />
          {m.upload_result_upload_another()}
        </Button>

        <Button
          aria-label={m.upload_result_delete()}
          className="shrink-0 border-current/25 text-current hover:bg-destructive/20 hover:text-destructive"
          onClick={() => setConfirmingDelete(true)}
          size="icon"
          type="button"
          variant="outline"
        >
          <Trash2 />
        </Button>
        <ResponsiveDialog
          alert
          onOpenChange={setConfirmingDelete}
          open={confirmingDelete}
        >
          <ResponsiveDialogContent>
            <ResponsiveDialogHeader>
              <ResponsiveDialogTitle>
                {m.upload_result_delete_confirm_title()}
              </ResponsiveDialogTitle>
              <ResponsiveDialogDescription>
                {m.upload_result_delete_confirm_description()}
              </ResponsiveDialogDescription>
            </ResponsiveDialogHeader>
            <ResponsiveDialogFooter>
              <Button
                onClick={() => setConfirmingDelete(false)}
                type="button"
                variant="outline"
              >
                {m.upload_result_cancel()}
              </Button>
              <Button
                disabled={deleting}
                onClick={() => {
                  setConfirmingDelete(false);
                  handleDelete();
                }}
                type="button"
                variant="destructive"
              >
                {m.upload_result_delete()}
              </Button>
            </ResponsiveDialogFooter>
          </ResponsiveDialogContent>
        </ResponsiveDialog>
      </div>
    </motion.div>
  );
}
