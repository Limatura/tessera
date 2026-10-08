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
import { Field, FieldGroup, FieldLabel } from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import { useState } from "react";
import { Controller, useWatch } from "react-hook-form";

import { readSetting } from "@/hooks/admin/use-settings";
import { useSettingsForm } from "@/hooks/admin/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/admin/use-settings-form";
import { featureSettingsSchema } from "@/lib/admin/settings-schema";
import type { FeatureSettingsValues } from "@/lib/admin/settings-schema";
import { asBoolean } from "@/lib/admin/settings-values";
import * as m from "@/paraglide/messages";

import { SettingsNotice, SwitchField } from "./form-fields";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";
import { SettingsCard } from "./settings-page";

type Settings = components["schemas"]["SettingsResponse"];

const KEYS = {
  accounts: "features.accounts",
  admin: "features.admin",
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
    <form className="flex flex-col gap-6" onSubmit={onSubmit}>
      <LockoutConfirmDialog
        keys={lockingOut}
        onConfirm={() => {
          setConfirmOpen(false);
          form.submit();
        }}
        onOpenChange={setConfirmOpen}
        open={confirmOpen}
      />

      <SettingsCard
        description={m.settings_feature_card_description()}
        title={m.settings_feature_card_title()}
      >
        <FieldGroup>
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
            <SettingsNotice
              icon={<IconAlertTriangle />}
              title={m.settings_feature_form_accounts_off_warning()}
              tone="warning"
            />
          )}

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

          {!admin && <AdminLockoutWarning />}
        </FieldGroup>
      </SettingsCard>

      <SettingsFormFooter form={form} />
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
    <SettingsNotice
      icon={<IconAlertTriangle />}
      title={m.settings_feature_form_admin_lockout_title()}
      tone="danger"
    >
      <p>
        {m.settings_feature_form_admin_lockout_description()}{" "}
        <code className="break-all">
          tessera-server settings reset features.admin
        </code>
      </p>
      <p className="mt-1">{m.settings_feature_form_lockout_docker_hint()}</p>
    </SettingsNotice>
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
