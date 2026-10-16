import { zodResolver } from "@hookform/resolvers/zod";
import type { components } from "@tessera/api/schema";
import { FieldGroup } from "@tessera/ui/components/field";
import { Controller } from "react-hook-form";
import type {
  Control,
  ControllerFieldState,
  ControllerRenderProps,
} from "react-hook-form";

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

import { SettingRow } from "./form-fields";
import { NumberField, NumberInput } from "./number-field";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";
import { SettingsCard } from "./settings-page";

type Settings = components["schemas"]["SettingsResponse"];

const DEFAULT_MAX_TRACKED_KEYS = 10_000;

const MINUTE = 60;
const HOUR = 60 * MINUTE;

/**
 * Defaults for each counter, matching the constants in `crate::auth::throttle`:
 * MAX_ATTEMPTS / WINDOW, SOURCE_*, LOOKUP_MAX_ATTEMPTS, RESET_*, SPEED_MAX_TESTS,
 * PWNED_MAX_LOOKUPS.
 */
const COUNTER_DEFAULTS: Record<
  ThrottleCounter,
  { maxAttempts: number; windowSecs: number }
> = {
  login: { maxAttempts: 10, windowSecs: 15 * MINUTE },
  lookup: { maxAttempts: 60, windowSecs: MINUTE },
  pwned: { maxAttempts: 30, windowSecs: MINUTE },
  registration: { maxAttempts: 3, windowSecs: HOUR },
  reset: { maxAttempts: 3, windowSecs: HOUR },
  share: { maxAttempts: 10, windowSecs: 15 * MINUTE },
  source: { maxAttempts: 120, windowSecs: MINUTE },
  speed: { maxAttempts: 5, windowSecs: HOUR },
  verification: { maxAttempts: 3, windowSecs: HOUR },
};

/**
 * Groups on screen: machine-wide limits first, then by what each protects.
 *
 * A function instead of a module-level const: the `m.xxx()` calls must read the
 * locale at render time, and a const would freeze the language at first load.
 */
function counterGroups(): {
  title: string;
  description: string;
  counters: ThrottleCounter[];
}[] {
  return [
    {
      counters: ["source"],
      description: m.settings_throttle_group_source_description(),
      title: m.settings_throttle_group_source_title(),
    },
    {
      counters: ["login", "share", "lookup"],
      description: m.settings_throttle_group_guessing_description(),
      title: m.settings_throttle_group_guessing_title(),
    },
    {
      counters: ["reset", "registration", "verification"],
      description: m.settings_throttle_group_mail_description(),
      title: m.settings_throttle_group_mail_title(),
    },
    {
      counters: ["speed", "pwned"],
      description: m.settings_throttle_group_other_description(),
      title: m.settings_throttle_group_other_title(),
    },
  ];
}

function counterCopy(): Record<
  ThrottleCounter,
  { title: string; description: string }
> {
  return {
    login: {
      description: m.settings_throttle_form_login_protects(),
      title: m.settings_throttle_form_login_title(),
    },
    lookup: {
      description: m.settings_throttle_form_lookup_protects(),
      title: m.settings_throttle_form_lookup_title(),
    },
    pwned: {
      description: m.settings_throttle_form_pwned_protects(),
      title: m.settings_throttle_form_pwned_title(),
    },
    registration: {
      description: m.settings_throttle_form_registration_protects(),
      title: m.settings_throttle_form_registration_title(),
    },
    reset: {
      description: m.settings_throttle_form_reset_protects(),
      title: m.settings_throttle_form_reset_title(),
    },
    share: {
      description: m.settings_throttle_form_share_protects(),
      title: m.settings_throttle_form_share_title(),
    },
    source: {
      description: m.settings_throttle_form_source_protects(),
      title: m.settings_throttle_form_source_title(),
    },
    speed: {
      description: m.settings_throttle_form_speed_protects(),
      title: m.settings_throttle_form_speed_title(),
    },
    verification: {
      description: m.settings_throttle_form_verification_protects(),
      title: m.settings_throttle_form_verification_title(),
    },
  };
}

const KEYS = {
  loginMaxAttempts: "auth.throttle.login.max_attempts",
  loginWindowSecs: "auth.throttle.login.window_secs",
  lookupMaxAttempts: "auth.throttle.lookup.max_attempts",
  lookupWindowSecs: "auth.throttle.lookup.window_secs",
  maxTrackedKeys: "auth.throttle.max_tracked_keys",
  pwnedMaxAttempts: "auth.throttle.pwned.max_attempts",
  pwnedWindowSecs: "auth.throttle.pwned.window_secs",
  registrationMaxAttempts: "auth.throttle.registration.max_attempts",
  registrationWindowSecs: "auth.throttle.registration.window_secs",
  resetMaxAttempts: "auth.throttle.reset.max_attempts",
  resetWindowSecs: "auth.throttle.reset.window_secs",
  shareMaxAttempts: "auth.throttle.share.max_attempts",
  shareWindowSecs: "auth.throttle.share.window_secs",
  sourceMaxAttempts: "auth.throttle.source.max_attempts",
  sourceWindowSecs: "auth.throttle.source.window_secs",
  speedMaxAttempts: "auth.throttle.speed.max_attempts",
  speedWindowSecs: "auth.throttle.speed.window_secs",
  verificationMaxAttempts: "auth.throttle.verification.max_attempts",
  verificationWindowSecs: "auth.throttle.verification.window_secs",
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
    <form className="flex flex-col gap-6" onSubmit={form.submit}>
      {counterGroups().map((group) => (
        <SettingsCard
          description={group.description}
          key={group.title}
          title={group.title}
        >
          <FieldGroup>
            {group.counters.map((counter) => (
              <CounterRow
                control={form.control}
                counter={counter}
                key={counter}
              />
            ))}
          </FieldGroup>
        </SettingsCard>
      ))}

      <SettingsCard
        description={m.settings_throttle_group_advanced_description()}
        title={m.settings_throttle_group_advanced_title()}
      >
        <FieldGroup>
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
        </FieldGroup>
      </SettingsCard>

      <SettingsFormFooter form={form} />
    </form>
  );
}

/**
 * One row per counter: what it protects on the left, count and window side by
 * side on the right. Errors for both go under the description, like other
 * setting rows.
 */
function CounterRow({
  control,
  counter,
}: {
  control: Control<ThrottleSettingsValues>;
  counter: ThrottleCounter;
}) {
  return (
    <Controller
      control={control}
      name={attemptsField(counter)}
      render={({ field: attempts, fieldState: attemptsState }) => (
        <Controller
          control={control}
          name={windowField(counter)}
          render={({ field: window, fieldState: windowState }) => (
            <CounterRowBody
              attempts={attempts}
              attemptsState={attemptsState}
              counter={counter}
              window={window}
              windowState={windowState}
            />
          )}
        />
      )}
    />
  );
}

/** The row's layout, split out so the two nested `Controller`s stay readable. */
function CounterRowBody({
  counter,
  attempts,
  attemptsState,
  window,
  windowState,
}: {
  counter: ThrottleCounter;
  attempts: ControllerRenderProps<ThrottleSettingsValues>;
  attemptsState: ControllerFieldState;
  window: ControllerRenderProps<ThrottleSettingsValues>;
  windowState: ControllerFieldState;
}) {
  const copy = counterCopy()[counter];

  return (
    <SettingRow
      controlClassName="@md/field-group:w-88"
      description={copy.description}
      error={attemptsState.error ?? windowState.error}
      id={`throttle-${counter}-attempts`}
      label={copy.title}
    >
      <div className="grid grid-cols-[7rem_1fr] gap-2">
        <NumberInput
          ariaLabel={m.settings_throttle_form_attempts_aria({
            name: copy.title,
          })}
          field={attempts}
          id={`throttle-${counter}-attempts`}
          invalid={attemptsState.invalid}
          min={1}
          unit="times"
        />
        <NumberInput
          ariaLabel={m.settings_throttle_form_window_aria({
            name: copy.title,
          })}
          field={window}
          id={`throttle-${counter}-window`}
          invalid={windowState.invalid}
          min={1}
          unit="seconds"
        />
      </div>
    </SettingRow>
  );
}
