import { IconAlertTriangle, IconCheck } from "@tabler/icons-react";
import { useBlocker } from "@tanstack/react-router";
import { Button } from "@tessera/ui/components/button";
import {
  ResponsiveDialog,
  ResponsiveDialogContent,
  ResponsiveDialogDescription,
  ResponsiveDialogFooter,
  ResponsiveDialogHeader,
  ResponsiveDialogTitle,
} from "@tessera/ui/components/responsive-dialog";
import { cn } from "@tessera/ui/lib/utils";

import type { Consequence } from "@/lib/admin/consequences";
import { consequenceText } from "@/lib/admin/consequences";
import * as m from "@/paraglide/messages";

import { SettingsNotice } from "./form-fields";

const SURFACE_CLASS = {
  card: "-mx-5 -mb-5 rounded-b-xl bg-card px-5",
  page: "bg-background/95 supports-[backdrop-filter]:bg-background/80 supports-[backdrop-filter]:backdrop-blur",
  sheet:
    "-mx-4 mt-6 bg-popover px-4 pb-[max(0.75rem,env(safe-area-inset-bottom))] md:-mx-6 md:px-6",
} as const;

/** The part of `useSettingsForm`'s return value this footer needs. */
interface FooterForm {
  canWrite: boolean;
  isDirty: boolean;
  isPending: boolean;
  isSaved: boolean;
  errorMessage: string | null;
  discard: () => void;
  /** Set while a save waits for the admin to accept what it deletes. */
  confirmation?: {
    consequences: Consequence[];
    isPending: boolean;
    confirm: () => void;
    cancel: () => void;
  } | null;
}

/**
 * The save bar at the bottom of every settings form.
 *
 * Sticky, so a change at the top of a long form can be saved without scrolling
 * to the end. With nothing changed it is a quiet status line; changes reveal
 * Discard and Save.
 *
 * Leaving the page with unsaved changes (sidebar, back, closing the tab) asks
 * first: filling in a whole page and losing it is the most common complaint.
 *
 * `surface="sheet"` is for the provider side panel, whose scroll area has no
 * bottom padding so the bar sits flush with the screen's bottom edge.
 *
 * `surface="card"` is for forms inside a card (the sign-in providers page): the
 * background matches the card, otherwise a differently coloured band shows
 * while it sticks.
 */
export function SettingsFormFooter({
  form,
  surface = "page",
}: {
  form: FooterForm;
  surface?: "page" | "card" | "sheet";
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
          "sticky bottom-0 z-10 flex flex-wrap items-center justify-end gap-x-3 gap-y-2 border-t py-3 max-md:*:data-[slot=button]:h-10",
          SURFACE_CLASS[surface]
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

      <ResponsiveDialog
        alert
        onOpenChange={(open) => {
          if (!open) {
            blocker.reset?.();
          }
        }}
        open={blocker.status === "blocked"}
      >
        <ResponsiveDialogContent>
          <ResponsiveDialogHeader>
            <ResponsiveDialogTitle>
              {m.settings_form_footer_leave_title()}
            </ResponsiveDialogTitle>
            <ResponsiveDialogDescription>
              {m.settings_form_footer_leave_description()}
            </ResponsiveDialogDescription>
          </ResponsiveDialogHeader>
          <ResponsiveDialogFooter>
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
          </ResponsiveDialogFooter>
        </ResponsiveDialogContent>
      </ResponsiveDialog>

      <ConfirmChangesDialog
        confirmation={form.confirmation ?? null}
        errorMessage={form.errorMessage}
      />
    </>
  );
}

/**
 * The sentence on the left of the save bar: why it cannot save, what failed,
 * unsaved changes, or just saved.
 */
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

/**
 * Shown when the server refuses a save until it is confirmed: a shorter
 * retention period applies to what is already stored, and the next sweep
 * deletes it. The numbers come from the server's preview, since "are you
 * sure?" with no numbers is a button everyone clicks.
 *
 * Cancel keeps the edits in the form, unsaved.
 */
function ConfirmChangesDialog({
  confirmation,
  errorMessage,
}: {
  confirmation: FooterForm["confirmation"];
  errorMessage: string | null;
}) {
  return (
    <ResponsiveDialog
      alert
      onOpenChange={(open) => {
        if (!open) {
          confirmation?.cancel();
        }
      }}
      open={Boolean(confirmation)}
    >
      <ResponsiveDialogContent>
        {confirmation && (
          <>
            <ResponsiveDialogHeader>
              <ResponsiveDialogTitle>
                {m.settings_confirm_title()}
              </ResponsiveDialogTitle>
              <ResponsiveDialogDescription>
                {m.settings_confirm_description()}
              </ResponsiveDialogDescription>
            </ResponsiveDialogHeader>
            <SettingsNotice icon={<IconAlertTriangle />} tone="danger">
              <ul className="flex flex-col gap-1">
                {confirmation.consequences.map((consequence) => (
                  <li key={consequence.key}>{consequenceText(consequence)}</li>
                ))}
              </ul>
            </SettingsNotice>
            {errorMessage && (
              <p className="text-destructive text-sm" role="alert">
                {errorMessage}
              </p>
            )}
            <ResponsiveDialogFooter>
              <Button
                onClick={confirmation.cancel}
                type="button"
                variant="outline"
              >
                {m.common_cancel()}
              </Button>
              <Button
                disabled={confirmation.isPending}
                onClick={confirmation.confirm}
                type="button"
                variant="destructive"
              >
                {confirmation.isPending
                  ? m.settings_form_footer_saving()
                  : m.settings_confirm_save()}
              </Button>
            </ResponsiveDialogFooter>
          </>
        )}
      </ResponsiveDialogContent>
    </ResponsiveDialog>
  );
}
