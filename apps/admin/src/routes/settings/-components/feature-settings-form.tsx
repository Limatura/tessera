import { zodResolver } from "@hookform/resolvers/zod";
import { IconAlertTriangle } from "@tabler/icons-react";
import type { components } from "@tessera/api/schema";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@tessera/ui/components/alert-dialog";
import { Button } from "@tessera/ui/components/button";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  FieldSeparator,
} from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import { Textarea } from "@tessera/ui/components/textarea";
import { useState } from "react";
import { Controller, useWatch } from "react-hook-form";

import { readSetting } from "@/hooks/use-settings";
import { useSettingsForm } from "@/hooks/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/use-settings-form";
import { featureSettingsSchema } from "@/lib/settings-schema";
import type { FeatureSettingsValues } from "@/lib/settings-schema";
import { asBoolean, asString } from "@/lib/settings-values";
import * as m from "@/paraglide/messages";

import { SwitchField } from "./form-fields";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";

type Settings = components["schemas"]["SettingsResponse"];

const KEYS = {
  accounts: "features.accounts",
  admin: "features.admin",
  ogLayout: "og.layout",
} satisfies SettingsKeyMap<FeatureSettingsValues>;

export function FeatureSettingsForm() {
  return (
    <SettingsLoader>
      {(settings) => (
        <FeatureSettingsFields defaultValues={initialValues(settings)} />
      )}
    </SettingsLoader>
  );
}

function initialValues(settings: Settings): FeatureSettingsValues {
  return {
    accounts: asBoolean(readSetting(settings, KEYS.accounts), true),
    admin: asBoolean(readSetting(settings, KEYS.admin), true),
    ogLayout: asString(readSetting(settings, KEYS.ogLayout), ""),
  };
}

function FeatureSettingsFields({
  defaultValues,
}: {
  defaultValues: FeatureSettingsValues;
}) {
  const form = useSettingsForm<FeatureSettingsValues>({
    defaultValues,
    keys: KEYS,
    resolver: zodResolver(featureSettingsSchema),
  });

  const admin = useWatch({ control: form.control, name: "admin" });
  const accounts = useWatch({ control: form.control, name: "accounts" });
  const [confirmOpen, setConfirmOpen] = useState(false);

  // 這次存檔會把哪些開關從開變成關 —— 兩個關掉都會進不了後台（#378）。
  const lockingOut = [
    defaultValues.admin && !admin ? KEYS.admin : null,
    defaultValues.accounts && !accounts ? KEYS.accounts : null,
  ].filter((key): key is string => key !== null);

  const onSubmit = (event: React.FormEvent) => {
    if (lockingOut.length > 0) {
      event.preventDefault();
      setConfirmOpen(true);
      return;
    }
    form.submit(event);
  };

  return (
    <form onSubmit={onSubmit}>
      <LockoutConfirmDialog
        keys={lockingOut}
        onConfirm={() => {
          setConfirmOpen(false);
          form.submit();
        }}
        onOpenChange={setConfirmOpen}
        open={confirmOpen}
      />
      <FieldGroup>
        <p className="text-muted-foreground text-sm">
          {m.settings_feature_form_intro()}
        </p>

        {!admin && <AdminLockoutWarning />}

        <Controller
          control={form.control}
          name="admin"
          render={({ field }) => (
            <SwitchField
              description={m.settings_feature_form_admin_description()}
              field={field}
              id="features-admin"
              label={m.settings_feature_form_admin_label()}
            />
          )}
        />

        <Controller
          control={form.control}
          name="accounts"
          render={({ field }) => (
            <SwitchField
              description={m.settings_feature_form_accounts_description()}
              field={field}
              id="features-accounts"
              label={m.settings_feature_form_accounts_label()}
            />
          )}
        />

        {!accounts && admin && (
          <p className="rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm">
            {m.settings_feature_form_accounts_off_warning()}
          </p>
        )}

        <FieldSeparator />

        <Controller
          control={form.control}
          name="ogLayout"
          render={({ field, fieldState }) => (
            <Field data-invalid={fieldState.invalid}>
              <FieldLabel htmlFor="features-og-layout">
                {m.settings_feature_form_og_layout_label()}
              </FieldLabel>
              <Textarea
                {...field}
                aria-invalid={fieldState.invalid}
                className="font-mono text-xs"
                id="features-og-layout"
                rows={6}
              />
              <FieldDescription>
                {m.settings_feature_form_og_layout_description_1()}{" "}
                <code>div</code>、<code>span</code>、<code>style</code>
                {m.settings_feature_form_og_layout_description_2()}{" "}
                <code>{"{{size}}"}</code>
                {m.settings_feature_form_og_layout_description_3()}
              </FieldDescription>
              {fieldState.invalid && <FieldError errors={[fieldState.error]} />}
            </Field>
          )}
        />

        <SettingsFormFooter
          canWrite={form.canWrite}
          errorMessage={form.errorMessage}
          isDirty={form.isDirty}
          isPending={form.isPending}
          isSaved={form.isSaved}
        />
      </FieldGroup>
    </form>
  );
}

/**
 * 關掉 `features.admin` 會把 operator 鎖在門外,而且面板自己救不回來。
 *
 * 後端允許這個寫入(它不在 FIXED_AT_BOOT),所以攔不住的只有「知不知道自己在做什麼」。
 * 存在資料庫的設定蓋過環境變數,所以救法是 CLI 的 `settings reset`,不是設環境變數。
 */
function AdminLockoutWarning() {
  return (
    <div
      className="flex gap-3 rounded-md border-2 border-destructive/50 bg-destructive/10 p-3 text-sm"
      role="alert"
    >
      <IconAlertTriangle className="mt-0.5 size-4 shrink-0 text-destructive" />
      <div className="space-y-1">
        <p className="font-medium">
          {m.settings_feature_form_admin_lockout_title()}
        </p>
        <p className="text-muted-foreground">
          {m.settings_feature_form_admin_lockout_description_1()}{" "}
          <code>tessera-server settings reset features.admin</code>{" "}
          {m.settings_feature_form_admin_lockout_description_2()}
        </p>
        <p className="text-muted-foreground">
          {m.settings_feature_form_lockout_docker_hint()}
        </p>
      </div>
    </div>
  );
}

/** 要打出這個字才能送出，避免一路按「下一步」就把自己鎖在外面。 */
const CONFIRM_WORD = "admin";

/**
 * 關掉會進不了後台的開關時，先跳這個確認。說明裡直接列出救回來要跑的指令，
 * 因為存檔之後這個畫面就再也看不到了。
 */
function LockoutConfirmDialog({
  keys,
  open,
  onOpenChange,
  onConfirm,
}: {
  keys: string[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
}) {
  const [typed, setTyped] = useState("");

  const handleOpenChange = (next: boolean) => {
    setTyped("");
    onOpenChange(next);
  };

  return (
    <AlertDialog onOpenChange={handleOpenChange} open={open}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            {m.settings_feature_form_lockout_confirm_title()}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {m.settings_feature_form_lockout_confirm_description()}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <pre className="overflow-x-auto rounded-md bg-muted p-3 text-xs">
          {keys.map((key) => `tessera-server settings reset ${key}`).join("\n")}
        </pre>
        <p className="text-muted-foreground text-xs">
          {m.settings_feature_form_lockout_docker_hint()}
        </p>
        <Field>
          <FieldLabel htmlFor="features-lockout-confirm">
            {m.settings_feature_form_lockout_confirm_label({
              word: CONFIRM_WORD,
            })}
          </FieldLabel>
          <Input
            autoComplete="off"
            id="features-lockout-confirm"
            onChange={(event) => setTyped(event.target.value)}
            value={typed}
          />
        </Field>
        <AlertDialogFooter>
          <Button
            onClick={() => handleOpenChange(false)}
            type="button"
            variant="outline"
          >
            {m.common_cancel()}
          </Button>
          <Button
            disabled={typed.trim() !== CONFIRM_WORD}
            onClick={() => {
              setTyped("");
              onConfirm();
            }}
            type="button"
            variant="destructive"
          >
            {m.settings_feature_form_lockout_confirm_action()}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
