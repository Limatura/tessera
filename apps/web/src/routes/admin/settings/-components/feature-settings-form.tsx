import { zodResolver } from "@hookform/resolvers/zod";
import { IconAlertTriangle } from "@tabler/icons-react";
import type { components } from "@tessera/api/schema";
import { Button } from "@tessera/ui/components/button";
import { Field, FieldGroup, FieldLabel } from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import {
  ResponsiveDialog,
  ResponsiveDialogContent,
  ResponsiveDialogDescription,
  ResponsiveDialogFooter,
  ResponsiveDialogHeader,
  ResponsiveDialogTitle,
} from "@tessera/ui/components/responsive-dialog";
import { useState } from "react";
import { Controller, useWatch } from "react-hook-form";

import { readSetting } from "@/hooks/admin/use-settings";
import { useSettingsForm } from "@/hooks/admin/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/admin/use-settings-form";
import { featureSettingsSchema } from "@/lib/admin/settings-schema";
import type { FeatureSettingsValues } from "@/lib/admin/settings-schema";
import { asBoolean, asNumber } from "@/lib/admin/settings-values";
import * as m from "@/paraglide/messages";

import { SettingsNotice, SwitchField } from "./form-fields";
import { NumberField } from "./number-field";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";
import { SettingsCard } from "./settings-page";

type Settings = components["schemas"]["SettingsResponse"];

/** `SpeedConfig::default` in config.rs: 25 MiB per test. */
const DEFAULT_SPEED_MAX_BYTES = 25 * 1024 * 1024;

const KEYS = {
  accounts: "features.accounts",
  admin: "features.admin",
  speedAnonymous: "speed.anonymous",
  speedEnabled: "speed.enabled",
  speedMaxBytes: "speed.max_bytes",
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
    speedAnonymous: asBoolean(
      readSetting(settings, KEYS.speedAnonymous),
      false
    ),
    speedEnabled: asBoolean(readSetting(settings, KEYS.speedEnabled), false),
    speedMaxBytes: asNumber(
      readSetting(settings, KEYS.speedMaxBytes),
      DEFAULT_SPEED_MAX_BYTES
    ),
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
  const speedEnabled = useWatch({
    control: form.control,
    name: "speedEnabled",
  });
  const [confirmOpen, setConfirmOpen] = useState(false);

  // Which switches this save turns off; turning off either one locks you out of the panel.
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

      <SettingsCard
        description={m.settings_speed_card_description()}
        title={m.settings_speed_card_title()}
      >
        <FieldGroup>
          <Controller
            control={form.control}
            name="speedEnabled"
            render={({ field }) => (
              <SwitchField
                description={m.settings_speed_form_enabled_description()}
                field={field}
                id="speed-enabled"
                label={m.settings_speed_form_enabled_label()}
              />
            )}
          />
          {speedEnabled && (
            <>
              <Controller
                control={form.control}
                name="speedAnonymous"
                render={({ field }) => (
                  <SwitchField
                    description={m.settings_speed_form_anonymous_description()}
                    field={field}
                    id="speed-anonymous"
                    label={m.settings_speed_form_anonymous_label()}
                  />
                )}
              />
              <Controller
                control={form.control}
                name="speedMaxBytes"
                render={({ field, fieldState }) => (
                  <NumberField
                    description={m.settings_speed_form_max_bytes_description()}
                    field={field}
                    fieldState={fieldState}
                    id="speed-max-bytes"
                    label={m.settings_speed_form_max_bytes_label()}
                    min={1}
                    unit="bytes"
                  />
                )}
              />
            </>
          )}
        </FieldGroup>
      </SettingsCard>

      <SettingsFormFooter form={form} />
    </form>
  );
}

/**
 * Turning off `features.admin` locks the operator out, and the panel cannot undo it.
 *
 * The backend accepts the write (it is not in FIXED_AT_BOOT), so all that stops it is
 * knowing what it does. Settings stored in the database override the environment, so
 * the recovery is the CLI's `settings reset`, not an environment variable.
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

/** Must be typed to submit, so clicking through does not lock you out. */
const CONFIRM_WORD = "admin";

/**
 * Confirmation before turning off a switch that locks you out of the panel. It lists
 * the recovery command, since this screen is unreachable after saving.
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
    <ResponsiveDialog alert onOpenChange={handleOpenChange} open={open}>
      <ResponsiveDialogContent>
        <ResponsiveDialogHeader>
          <ResponsiveDialogTitle>
            {m.settings_feature_form_lockout_confirm_title()}
          </ResponsiveDialogTitle>
          <ResponsiveDialogDescription>
            {m.settings_feature_form_lockout_confirm_description()}
          </ResponsiveDialogDescription>
        </ResponsiveDialogHeader>
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
        <ResponsiveDialogFooter>
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
        </ResponsiveDialogFooter>
      </ResponsiveDialogContent>
    </ResponsiveDialog>
  );
}
