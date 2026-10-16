import { IconAlertTriangle, IconRestore } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";
import type { components } from "@tessera/api/schema";
import { useSession } from "@tessera/auth/hooks";
import { Badge } from "@tessera/ui/components/badge";
import { Button, buttonVariants } from "@tessera/ui/components/button";
import {
  ResponsiveDialog,
  ResponsiveDialogContent,
  ResponsiveDialogDescription,
  ResponsiveDialogFooter,
  ResponsiveDialogHeader,
  ResponsiveDialogTitle,
} from "@tessera/ui/components/responsive-dialog";
import { toast } from "@tessera/ui/components/toast";
import { cn } from "@tessera/ui/lib/utils";
import { useEffect, useState } from "react";

import { ListError, ListSkeleton } from "@/components/admin/list-state";
import {
  clearing,
  usePreviewChanges,
  useResetSettings,
  useSettings,
} from "@/hooks/admin/use-settings";
import { consequenceText } from "@/lib/admin/consequences";
import { hasPermission, PERMISSIONS } from "@/lib/admin/permissions";
import { settingsNavGroups } from "@/lib/admin/settings-nav";
import { LOCKOUT_KEYS, settingsSectionOf } from "@/lib/admin/settings-sections";
import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

import { SettingsNotice } from "./form-fields";
import { SettingsCard } from "./settings-page";

type Setting = components["schemas"]["SettingResponse"];

interface Section {
  /** The settings page's URL, or `null` for keys no page edits. */
  url: string | null;
  title: string;
  keys: string[];
  secrets: number;
}

/** What a reset is about to clear: one section, or everything. */
interface Pending {
  title: string;
  keys: string[];
  secrets: number;
  all: boolean;
}

/**
 * Puts settings changed in the panel back to the server's own value (the
 * environment variable, or the built-in default) by clearing them.
 *
 * Grouped by the page each setting is edited on, so an admin can undo one
 * page's experiments without touching the rest, or all of it at once.
 */
export function ResetSettings() {
  const settings = useSettings();
  const { data: account } = useSession();
  const canWrite = hasPermission(account, PERMISSIONS.settingsWrite);
  const [pending, setPending] = useState<Pending | null>(null);

  if (settings.isPending) {
    return <ListSkeleton rows={3} />;
  }
  if (settings.isError) {
    return (
      <ListError
        isRetrying={settings.isFetching}
        message={m.settings_loader_error()}
        onRetry={() => settings.refetch()}
      />
    );
  }

  const changed = settings.data.settings.filter(
    (setting) => setting.source === "panel" && setting.editable
  );
  // Resetting these to an "off" fallback would lock the admin out of undoing it.
  const lockouts = changed.filter(
    (setting) =>
      LOCKOUT_KEYS.has(setting.key) && (setting.fallback as unknown) === false
  );
  const resettable = changed.filter((setting) => !lockouts.includes(setting));
  const sections = groupBySection(resettable);
  const total = resettable.length;
  const totalSecrets = resettable.filter((setting) => setting.secret).length;

  return (
    <>
      <SettingsNotice>{m.settings_reset_scope_note()}</SettingsNotice>

      {lockouts.map((setting) => (
        <SettingsNotice
          icon={<IconAlertTriangle />}
          key={setting.key}
          tone="warning"
        >
          {m.settings_reset_lockout({ name: lockoutName(setting.key) })}
        </SettingsNotice>
      ))}

      <SettingsCard
        action={
          canWrite &&
          total > 0 && (
            <Button
              onClick={() =>
                setPending({
                  all: true,
                  keys: resettable.map((setting) => setting.key),
                  secrets: totalSecrets,
                  title: "",
                })
              }
              size="sm"
              type="button"
              variant="destructive"
            >
              <IconRestore />
              {m.settings_reset_all({ count: total })}
            </Button>
          )
        }
        description={m.settings_reset_card_description()}
        title={m.settings_reset_card_title()}
      >
        {sections.length === 0 ? (
          <p className="py-6 text-center text-muted-foreground text-sm">
            {m.settings_reset_empty()}
          </p>
        ) : (
          <ul className="-my-2 flex flex-col divide-y">
            {sections.map((section) => (
              <SectionRow
                canWrite={canWrite}
                key={section.url ?? "other"}
                onReset={() =>
                  setPending({
                    all: false,
                    keys: section.keys,
                    secrets: section.secrets,
                    title: section.title,
                  })
                }
                section={section}
              />
            ))}
          </ul>
        )}
      </SettingsCard>

      <ResetDialog onClose={() => setPending(null)} pending={pending} />
    </>
  );
}

function SectionRow({
  section,
  canWrite,
  onReset,
}: {
  section: Section;
  canWrite: boolean;
  onReset: () => void;
}) {
  return (
    <li className="flex flex-wrap items-center gap-x-4 gap-y-2 py-3">
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <span className="font-medium">{section.title}</span>
        <span className="flex flex-wrap items-center gap-2 text-muted-foreground text-xs">
          {m.settings_reset_count({ count: section.keys.length })}
          {section.secrets > 0 && (
            <Badge variant="outline">
              {m.settings_reset_secrets({ count: section.secrets })}
            </Badge>
          )}
        </span>
      </div>
      <div className="flex shrink-0 items-center gap-2">
        {section.url && (
          <Link
            className={cn(buttonVariants({ size: "sm", variant: "ghost" }))}
            to={section.url}
          >
            {m.settings_reset_open()}
          </Link>
        )}
        {canWrite && (
          <Button onClick={onReset} size="sm" type="button" variant="outline">
            <IconRestore />
            {m.settings_reset_section()}
          </Button>
        )}
      </div>
    </li>
  );
}

/**
 * Confirms a reset. On open it asks the server what the reset would destroy
 * (a retention period going back to a shorter default, say) and shows the
 * numbers, since only those give an admin pause; the reset is then sent with
 * `confirm`.
 */
function ResetDialog({
  pending,
  onClose,
}: {
  pending: Pending | null;
  onClose: () => void;
}) {
  const preview = usePreviewChanges();
  const reset = useResetSettings();
  const keys = pending?.keys;
  // `mutate` is stable across renders; the preview runs once per selection.
  const { mutate: runPreview } = preview;

  useEffect(() => {
    if (keys) {
      runPreview(clearing(keys));
    }
  }, [keys, runPreview]);

  const handleOpenChange = (open: boolean) => {
    if (!open) {
      preview.reset();
      reset.reset();
      onClose();
    }
  };

  const handleConfirm = () => {
    if (!pending) {
      return;
    }
    reset.mutate(pending.keys, {
      onSuccess: () => {
        toast.add({
          title: m.settings_reset_success({ count: pending.keys.length }),
          type: "success",
        });
        handleOpenChange(false);
      },
    });
  };

  const error = preview.error ?? reset.error;

  return (
    <ResponsiveDialog
      alert
      onOpenChange={handleOpenChange}
      open={pending !== null}
    >
      <ResponsiveDialogContent>
        {pending && (
          <>
            <ResponsiveDialogHeader>
              <ResponsiveDialogTitle>
                {pending.all
                  ? m.settings_reset_dialog_title_all({
                      count: pending.keys.length,
                    })
                  : m.settings_reset_dialog_title({
                      count: pending.keys.length,
                      section: pending.title,
                    })}
              </ResponsiveDialogTitle>
              <ResponsiveDialogDescription>
                {m.settings_reset_dialog_description()}
              </ResponsiveDialogDescription>
            </ResponsiveDialogHeader>

            {pending.secrets > 0 && (
              <SettingsNotice icon={<IconAlertTriangle />} tone="warning">
                {m.settings_reset_dialog_secrets({ count: pending.secrets })}
              </SettingsNotice>
            )}
            {preview.isPending && (
              <p className="text-muted-foreground text-sm">
                {m.settings_reset_dialog_checking()}
              </p>
            )}
            {preview.data && preview.data.length > 0 && (
              <SettingsNotice icon={<IconAlertTriangle />} tone="danger">
                <ul className="flex flex-col gap-1">
                  {preview.data.map((consequence) => (
                    <li key={consequence.key}>
                      {consequenceText(consequence)}
                    </li>
                  ))}
                </ul>
              </SettingsNotice>
            )}
            {error && (
              <p className="text-destructive text-sm" role="alert">
                {apiErrorMessage(error, m.settings_form_save_failed())}
              </p>
            )}

            <ResponsiveDialogFooter>
              <Button
                onClick={() => handleOpenChange(false)}
                type="button"
                variant="outline"
              >
                {m.common_cancel()}
              </Button>
              <Button
                disabled={
                  preview.isPending || preview.isError || reset.isPending
                }
                onClick={handleConfirm}
                type="button"
                variant="destructive"
              >
                {reset.isPending
                  ? m.settings_reset_pending()
                  : m.settings_reset_confirm()}
              </Button>
            </ResponsiveDialogFooter>
          </>
        )}
      </ResponsiveDialogContent>
    </ResponsiveDialog>
  );
}

/** Sections in the settings menu's order, then "other" last. */
function groupBySection(settings: Setting[]): Section[] {
  const order = settingsNavGroups().flatMap((group) => group.items);
  const sections = new Map<string | null, Section>();

  for (const setting of settings) {
    const url = settingsSectionOf(setting.key);
    const section = sections.get(url) ?? {
      keys: [],
      secrets: 0,
      title:
        order.find((item) => item.url === url)?.title ??
        m.settings_reset_other(),
      url,
    };
    section.keys.push(setting.key);
    section.secrets += setting.secret ? 1 : 0;
    sections.set(url, section);
  }

  const rank = (url: string | null) => {
    const index = order.findIndex((item) => item.url === url);
    return index === -1 ? order.length : index;
  };
  return [...sections.values()].toSorted((a, b) => rank(a.url) - rank(b.url));
}

function lockoutName(key: string): string {
  return key === "features.admin"
    ? m.settings_reset_name_admin()
    : m.settings_reset_name_accounts();
}
