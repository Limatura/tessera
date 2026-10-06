import { zodResolver } from "@hookform/resolvers/zod";
import type { components } from "@tessera/api/schema";
import {
  FieldGroup,
  FieldLegend,
  FieldSeparator,
  FieldSet,
} from "@tessera/ui/components/field";
import { Controller } from "react-hook-form";
import type { Control } from "react-hook-form";

import { readSetting } from "@/hooks/admin/use-settings";
import { useSettingsForm } from "@/hooks/admin/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/admin/use-settings-form";
import {
  MAX_TRACKED_KEYS_CEILING,
  THROTTLE_COUNTERS,
  throttleSettingsSchema,
} from "@/lib/admin/settings-schema";
import type {
  ThrottleCounter,
  ThrottleSettingsValues,
} from "@/lib/admin/settings-schema";
import { asNumber } from "@/lib/admin/settings-values";
import * as m from "@/paraglide/messages";

import { NumberField } from "./number-field";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";

type Settings = components["schemas"]["SettingsResponse"];

const DEFAULT_MAX_TRACKED_KEYS = 10_000;

/** 每組計數器的預設值。 */
const COUNTER_DEFAULTS: Record<
  ThrottleCounter,
  { maxAttempts: number; windowSecs: number }
> = {
  // 對應 `crate::auth::throttle` 的常數：MAX_ATTEMPTS / WINDOW、SOURCE_*、
  // LOOKUP_MAX_ATTEMPTS（視窗沿用 SOURCE_WINDOW）、RESET_*。
  login: { maxAttempts: 10, windowSecs: 15 * 60 },
  lookup: { maxAttempts: 60, windowSecs: 60 },
  registration: { maxAttempts: 3, windowSecs: 60 * 60 },
  reset: { maxAttempts: 3, windowSecs: 60 * 60 },
  share: { maxAttempts: 10, windowSecs: 15 * 60 },
  source: { maxAttempts: 120, windowSecs: 60 },
};

/**
 * 每組計數器在畫面上的說明。
 *
 * 「計數依據」是重點 —— 同樣一個 5 次,以帳號計跟以來源位址計是完全不同的影響範圍。
 *
 * 是函式而不是 module-level const —— 裡面呼叫的 `m.xxx()` 要在渲染當下讀 locale,
 * 包成 const 只會在模組第一次載入時凍住語系。
 */
function counterCopy(): Record<
  ThrottleCounter,
  { title: string; keyedOn: string; protects: string }
> {
  return {
    login: {
      keyedOn: m.settings_throttle_form_login_keyed_on(),
      protects: m.settings_throttle_form_login_protects(),
      title: m.settings_throttle_form_login_title(),
    },
    lookup: {
      keyedOn: m.settings_throttle_form_lookup_keyed_on(),
      protects: m.settings_throttle_form_lookup_protects(),
      title: m.settings_throttle_form_lookup_title(),
    },
    registration: {
      keyedOn: m.settings_throttle_form_registration_keyed_on(),
      protects: m.settings_throttle_form_registration_protects(),
      title: m.settings_throttle_form_registration_title(),
    },
    reset: {
      keyedOn: m.settings_throttle_form_reset_keyed_on(),
      protects: m.settings_throttle_form_reset_protects(),
      title: m.settings_throttle_form_reset_title(),
    },
    share: {
      keyedOn: m.settings_throttle_form_share_keyed_on(),
      protects: m.settings_throttle_form_share_protects(),
      title: m.settings_throttle_form_share_title(),
    },
    source: {
      keyedOn: m.settings_throttle_form_source_keyed_on(),
      protects: m.settings_throttle_form_source_protects(),
      title: m.settings_throttle_form_source_title(),
    },
  };
}

const KEYS = {
  loginMaxAttempts: "auth.throttle.login.max_attempts",
  loginWindowSecs: "auth.throttle.login.window_secs",
  lookupMaxAttempts: "auth.throttle.lookup.max_attempts",
  lookupWindowSecs: "auth.throttle.lookup.window_secs",
  maxTrackedKeys: "auth.throttle.max_tracked_keys",
  registrationMaxAttempts: "auth.throttle.registration.max_attempts",
  registrationWindowSecs: "auth.throttle.registration.window_secs",
  resetMaxAttempts: "auth.throttle.reset.max_attempts",
  resetWindowSecs: "auth.throttle.reset.window_secs",
  shareMaxAttempts: "auth.throttle.share.max_attempts",
  shareWindowSecs: "auth.throttle.share.window_secs",
  sourceMaxAttempts: "auth.throttle.source.max_attempts",
  sourceWindowSecs: "auth.throttle.source.window_secs",
} satisfies SettingsKeyMap<ThrottleSettingsValues>;

export function ThrottleSettingsForm() {
  return (
    <SettingsLoader>
      {(settings) => (
        <ThrottleSettingsFields defaultValues={initialValues(settings)} />
      )}
    </SettingsLoader>
  );
}

function attemptsField(counter: ThrottleCounter) {
  return `${counter}MaxAttempts` as keyof ThrottleSettingsValues & string;
}

function windowField(counter: ThrottleCounter) {
  return `${counter}WindowSecs` as keyof ThrottleSettingsValues & string;
}

function initialValues(settings: Settings): ThrottleSettingsValues {
  const values = {
    maxTrackedKeys: asNumber(
      readSetting(settings, KEYS.maxTrackedKeys),
      DEFAULT_MAX_TRACKED_KEYS
    ),
  } as ThrottleSettingsValues;

  for (const counter of THROTTLE_COUNTERS) {
    const attempts = attemptsField(counter);
    const window = windowField(counter);
    values[attempts] = asNumber(
      readSetting(settings, KEYS[attempts]),
      COUNTER_DEFAULTS[counter].maxAttempts
    );
    values[window] = asNumber(
      readSetting(settings, KEYS[window]),
      COUNTER_DEFAULTS[counter].windowSecs
    );
  }

  return values;
}

function ThrottleSettingsFields({
  defaultValues,
}: {
  defaultValues: ThrottleSettingsValues;
}) {
  const form = useSettingsForm<ThrottleSettingsValues>({
    defaultValues,
    keys: KEYS,
    resolver: zodResolver(throttleSettingsSchema),
  });

  return (
    <form onSubmit={form.submit}>
      <FieldGroup>
        <p className="text-muted-foreground text-sm">
          {m.settings_throttle_form_intro()}
        </p>

        {THROTTLE_COUNTERS.map((counter) => (
          <CounterFields
            control={form.control}
            counter={counter}
            key={counter}
          />
        ))}

        <FieldSeparator />

        <Controller
          control={form.control}
          name="maxTrackedKeys"
          render={({ field, fieldState }) => (
            <NumberField
              description={m.settings_throttle_form_max_tracked_keys_description()}
              field={field}
              fieldState={fieldState}
              id="throttle-max-tracked-keys"
              label={m.settings_throttle_form_max_tracked_keys_label()}
              max={MAX_TRACKED_KEYS_CEILING}
              min={1}
            />
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

function CounterFields({
  control,
  counter,
}: {
  control: Control<ThrottleSettingsValues>;
  counter: ThrottleCounter;
}) {
  const copy = counterCopy()[counter];

  return (
    <FieldSet>
      <FieldLegend variant="label">
        {copy.title}
        <span className="ml-2 font-normal text-muted-foreground">
          {m.settings_throttle_form_keyed_on_suffix({ keyedOn: copy.keyedOn })}
        </span>
      </FieldLegend>
      <p className="text-muted-foreground text-sm">{copy.protects}</p>
      <div className="grid gap-4 sm:grid-cols-2">
        <Controller
          control={control}
          name={attemptsField(counter)}
          render={({ field, fieldState }) => (
            <NumberField
              field={field}
              fieldState={fieldState}
              id={`throttle-${counter}-attempts`}
              label={m.settings_throttle_form_attempts_label()}
              min={1}
            />
          )}
        />
        <Controller
          control={control}
          name={windowField(counter)}
          render={({ field, fieldState }) => (
            <NumberField
              field={field}
              fieldState={fieldState}
              id={`throttle-${counter}-window`}
              label={m.settings_throttle_form_window_label()}
              min={1}
            />
          )}
        />
      </div>
    </FieldSet>
  );
}
