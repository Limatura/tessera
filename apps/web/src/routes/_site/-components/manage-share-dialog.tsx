import { Button } from "@tessera/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@tessera/ui/components/dialog";
import { toast } from "@tessera/ui/components/toast";
import { Check, Copy, QrCode, Share2, Trash2 } from "lucide-react";
import { QRCodeSVG } from "qrcode.react";
import { useState } from "react";
import type { ReactNode } from "react";

import { useShare } from "@/hooks/use-share";
import { apiErrorMessage } from "@/lib/api-error";
import { deleteShare, minutesUntil } from "@/lib/upload/share-api";
import * as m from "@/paraglide/messages";
import { useShareStore } from "@/stores/share";

import { ShareSettingsPanel } from "./share-settings-panel";
import type { ShareSettingsSnapshot } from "./share-settings-panel";

/** 要管理的那一個分享。`initial` 是呼叫端手上最後知道的設定。 */
export interface ManagedShare {
  code: string;
  title: string;
  shortUrl: string;
  /** 匿名上傳才有;登入者自己的分享靠 session。 */
  managementToken?: string;
  initial: ShareSettingsSnapshot;
}

/**
 * 分享建立之後回來改設定或刪掉它(#364)。
 *
 * 「我的分享」跟首頁的「這台瀏覽器最近上傳」共用:前者手上的資料就是 server 剛給的,
 * 直接用;後者是存在瀏覽器裡的快照,可能早就過期或被刪,所以 `verify` 打開時先問
 * server 一次 —— 讀得到就以它為準,404/410 就說它沒了,只剩從清單移除可以做。
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
  /** 給了才出現「從清單移除」,只動這台瀏覽器的紀錄。 */
  onForget?: (code: string) => void;
}) {
  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      {/* grid 的隱含欄預設是 auto,長標題 / 長網址會把欄撐得比對話框寬,內容就溢出右邊;
          明確給 minmax(0,1fr) 才能讓 truncate 生效。 */}
      <DialogContent className="grid-cols-[minmax(0,1fr)] sm:max-w-md">
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
      </DialogContent>
    </Dialog>
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
 * 先向 server 確認還在不在,再掛上表單 —— 表單的初始值只讀一次,先掛再換會讓開關
 * 閃一下錯的狀態。
 *
 * 有密碼的分享沒帶密碼時,server 只回「它有密碼」,不給期限跟次數;這時沿用快照。
 */
function VerifiedShareContent(props: ContentProps) {
  const live = useShare(props.share.code);

  // 等這次打開之後的那一份 —— 快取裡可能是上次打開時的值,而表單只在掛上時讀一次,
  // 拿舊的掛上去,剛改過的期限會顯示成改之前的樣子。
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
        }
      : { ...props.share.initial, hasPassword: data?.has_password ?? true };

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

      <DialogFooter className="sm:justify-between">
        {footerActions}
        <Button onClick={onClose} type="button">
          {m.manage_share_done()}
        </Button>
      </DialogFooter>
    </>
  );
}

/** 分享已經過期、用完或被刪掉:沒什麼能改的,只剩把它從清單拿掉。 */
function GoneShareContent({ share, onClose, onForget }: ContentProps) {
  const [confirmingForget, setConfirmingForget] = useState(false);

  return (
    <>
      <ShareHeader share={share} />
      <p className="text-muted-foreground text-sm">{m.manage_share_gone()}</p>
      <DialogFooter>
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
      </DialogFooter>
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
    <DialogHeader>
      <DialogTitle className="truncate pr-6">{share.title}</DialogTitle>
      <DialogDescription className="flex items-center gap-1">
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
      </DialogDescription>
      {showQr && (
        // 深色碼、白底,暗色模式也一樣:反色的 QR 有些掃描器讀不到。
        <div className="mx-auto mt-2 w-fit rounded-xl bg-white p-3 ring-1 ring-foreground/10">
          <QRCodeSVG
            bgColor="#ffffff"
            fgColor="#0a0a0a"
            size={160}
            value={share.shortUrl}
          />
        </div>
      )}
    </DialogHeader>
  );
}
