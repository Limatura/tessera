import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@tessera/ui/components/alert-dialog";
import { Button } from "@tessera/ui/components/button";
import { toast } from "@tessera/ui/components/toast";
import { Check, Copy, RotateCcw, Trash2 } from "lucide-react";
import { motion } from "motion/react";
import { QRCodeSVG } from "qrcode.react";
import { useEffect, useState } from "react";

import { apiErrorMessage } from "@/lib/api-error";
import { deleteShare } from "@/lib/upload";
import type { UploadResultData } from "@/lib/upload";
import * as m from "@/paraglide/messages";
import { useRecentSharesStore } from "@/stores/recent-shares";

import { ShareSettingsPanel, expiresAtFrom } from "./share-settings-panel";
import type { ShareSettingsSnapshot } from "./share-settings-panel";

/**
 * 覆蓋層上的區塊底色。這層的底是 --melt 而不是 background/card,
 * 所以用 currentColor 的透明度堆出深淺——亮暗模式各自的 --melt-foreground
 * 會自動把它翻成「暗色壓深」或「亮色提亮」,不用寫成兩套 dark: 變體。
 */
const PANEL = "rounded-lg border border-current/15 bg-current/5";

interface UploadResultProps {
  onReset: () => void;
  result: UploadResultData;
  /** 收合動畫期間仍保持掛載,只把內容淡出,避免拆掉整棵子樹卡住動畫的第一幀 */
  visible: boolean;
}

export function UploadResult({ onReset, result, visible }: UploadResultProps) {
  const [copied, setCopied] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const remember = useRecentSharesStore((state) => state.remember);
  const updateRecent = useRecentSharesStore((state) => state.update);
  const forgetRecent = useRecentSharesStore((state) => state.forget);

  // 匿名上傳的管理 token 只在這個畫面拿得到,關掉就沒了 —— 記進這台瀏覽器的
  // 「最近上傳」,之後還能從首頁回來改(#364)。登入者的分享沒有 token,歸帳號管。
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
    await navigator.clipboard.writeText(result.shortUrl);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  const handleDelete = async () => {
    setDeleting(true);
    try {
      await deleteShare(result.code, result.managementToken);
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
      className="relative m-auto flex w-full max-w-sm flex-col items-center gap-6 text-center text-melt-foreground"
      initial={{ opacity: 0, y: 12 }}
      transition={
        visible
          ? { delay: 0.15, duration: 0.4, ease: [0.16, 1, 0.3, 1] }
          : { duration: 0.15, ease: "linear" }
      }
    >
      {/*
        QR 一律維持「深色碼、白底」:反色(亮碼暗底)有一部分掃描器讀不到。
        暗色模式下就讓這張白底自己當一張卡片,靠外圈的 ring 收邊,不要跟著變暗。
      */}
      <div className="rounded-xl bg-white p-3 ring-1 ring-current/15 dark:shadow-black/40 dark:shadow-lg">
        <QRCodeSVG
          bgColor="#ffffff"
          fgColor="#0a0a0a"
          size={160}
          value={result.shortUrl}
        />
      </div>

      <div className="flex flex-col gap-1">
        <p className="font-medium text-sm">{result.fileName}</p>
        <p className="text-current/70 text-xs">{m.upload_result_ready()}</p>
      </div>

      <div className={`flex w-full items-center gap-2 px-3 py-2 ${PANEL}`}>
        <span className="flex-1 truncate text-left font-mono text-xs">
          {result.shortUrl}
        </span>
        <Button
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

        <AlertDialog>
          <AlertDialogTrigger
            render={
              <Button
                aria-label={m.upload_result_delete()}
                className="shrink-0 border-current/25 text-current hover:bg-destructive/20 hover:text-destructive"
                size="icon"
                type="button"
                variant="outline"
              />
            }
          >
            <Trash2 />
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>
                {m.upload_result_delete_confirm_title()}
              </AlertDialogTitle>
              <AlertDialogDescription>
                {m.upload_result_delete_confirm_description()}
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>{m.upload_result_cancel()}</AlertDialogCancel>
              <AlertDialogAction
                disabled={deleting}
                onClick={handleDelete}
                variant="destructive"
              >
                {m.upload_result_delete()}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>
    </motion.div>
  );
}
