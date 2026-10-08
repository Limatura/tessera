import { IconCheck } from "@tabler/icons-react";
import { useBlocker } from "@tanstack/react-router";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@tessera/ui/components/alert-dialog";
import { Button } from "@tessera/ui/components/button";
import { cn } from "@tessera/ui/lib/utils";

import * as m from "@/paraglide/messages";

/** `useSettingsForm` 回傳值裡這一塊要用到的部分。 */
interface FooterForm {
  canWrite: boolean;
  isDirty: boolean;
  isPending: boolean;
  isSaved: boolean;
  errorMessage: string | null;
  discard: () => void;
}

/**
 * 每個設定表單最底下的儲存列。
 *
 * 黏在畫面底部：長的表單改了最上面一格，不用捲到底才找得到儲存。沒改東西時只是一條
 * 安靜的狀態列，有改動才把「捨棄」「儲存」亮出來。
 *
 * 有沒存的改動時，離開這一頁（點側欄、上一頁、關分頁）會先問一聲 —— 設定表單一次
 * 要填好幾格，整頁打完才發現沒存是最常見的抱怨。
 *
 * `surface="card"` 給放在卡片裡的表單（登入方式那頁）：底色要跟卡片一樣，不然黏住
 * 時會看到一條顏色不同的帶子。
 */
export function SettingsFormFooter({
  form,
  surface = "page",
}: {
  form: FooterForm;
  surface?: "page" | "card";
}) {
  const { canWrite, isDirty, isPending, isSaved, errorMessage, discard } = form;

  const blocker = useBlocker({
    enableBeforeUnload: isDirty,
    shouldBlockFn: () => isDirty,
    withResolver: true,
  });

  return (
    <>
      <div
        className={cn(
          "sticky bottom-0 z-10 flex flex-wrap items-center justify-end gap-x-3 gap-y-2 border-t py-3",
          surface === "page"
            ? "bg-background/95 supports-[backdrop-filter]:bg-background/80 supports-[backdrop-filter]:backdrop-blur"
            : "-mx-5 -mb-5 rounded-b-xl bg-card px-5"
        )}
      >
        <FooterStatus
          canWrite={canWrite}
          errorMessage={errorMessage}
          isDirty={isDirty}
          isSaved={isSaved}
        />
        {canWrite && isDirty && (
          <Button
            disabled={isPending}
            onClick={discard}
            type="button"
            variant="ghost"
          >
            {m.settings_form_footer_discard()}
          </Button>
        )}
        <Button disabled={!(canWrite && isDirty) || isPending} type="submit">
          {isPending
            ? m.settings_form_footer_saving()
            : m.settings_form_footer_save()}
        </Button>
      </div>

      <AlertDialog
        onOpenChange={(open) => {
          if (!open) {
            blocker.reset?.();
          }
        }}
        open={blocker.status === "blocked"}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {m.settings_form_footer_leave_title()}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {m.settings_form_footer_leave_description()}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <Button
              onClick={() => blocker.reset?.()}
              type="button"
              variant="outline"
            >
              {m.settings_form_footer_leave_stay()}
            </Button>
            <Button
              onClick={() => blocker.proceed?.()}
              type="button"
              variant="destructive"
            >
              {m.settings_form_footer_leave_discard()}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

/** 儲存列左邊那一句：為什麼不能存、哪裡出錯、還有沒存的改動，或剛存好。 */
function FooterStatus({
  canWrite,
  isDirty,
  isSaved,
  errorMessage,
}: {
  canWrite: boolean;
  isDirty: boolean;
  isSaved: boolean;
  errorMessage: string | null;
}) {
  const className = "mr-auto min-w-0 text-sm";

  if (!canWrite) {
    return (
      <p className={cn(className, "text-muted-foreground")}>
        {m.settings_form_footer_read_only()}
      </p>
    );
  }
  if (errorMessage) {
    return (
      <p className={cn(className, "text-destructive")} role="alert">
        {errorMessage}
      </p>
    );
  }
  if (isDirty) {
    return (
      <p className={cn(className, "text-muted-foreground")}>
        {m.settings_form_footer_unsaved()}
      </p>
    );
  }
  if (isSaved) {
    return (
      <output
        className={cn(
          className,
          "flex items-center gap-1.5 text-muted-foreground"
        )}
      >
        <IconCheck className="size-4 text-primary" />
        {m.settings_form_footer_saved()}
      </output>
    );
  }
  return null;
}
