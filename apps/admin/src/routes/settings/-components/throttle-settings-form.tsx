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

import { readSetting } from "@/hooks/use-settings";
import { useSettingsForm } from "@/hooks/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/use-settings-form";
import {
  MAX_TRACKED_KEYS_CEILING,
  THROTTLE_COUNTERS,
  throttleSettingsSchema,
} from "@/lib/settings-schema";
import type {
  ThrottleCounter,
  ThrottleSettingsValues,
} from "@/lib/settings-schema";
import { asNumber } from "@/lib/settings-values";

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
 */
const COUNTER_COPY: Record<
  ThrottleCounter,
  { title: string; keyedOn: string; protects: string }
> = {
  login: {
    keyedOn: "帳號 + 來源位址",
    protects: "失敗的登入嘗試。",
    title: "登入",
  },
  lookup: {
    keyedOn: "來源位址",
    protects:
      "查了不存在的取件碼。短取件碼的安全性完全靠這一組 —— 放寬它等於讓人有機會走訪完整個取件碼空間。",
    title: "取件碼查詢",
  },
  registration: {
    keyedOn: "被查詢的 email",
    protects: "註冊確認信的發送。",
    title: "註冊信",
  },
  reset: {
    keyedOn: "被查詢的 email",
    protects: "重設密碼信的發送。",
    title: "重設密碼信",
  },
  share: {
    keyedOn: "取件碼 + 來源位址",
    protects: "分享密碼猜錯的次數。",
    title: "分享密碼",
  },
  source: {
    keyedOn: "來源位址",
    protects:
      "任何昂貴操作。刻意寬鬆 —— 位址是共用的，擋一個等於擋掉它後面所有人。",
    title: "來源位址總量",
  },
};

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
          六組計數器保護不同的東西，所以分開設。兩個值都不能是 0：0
          次嘗試等於鎖死所有人 （在「來源位址總量」上就是整台機器），0
          秒視窗則永遠不會擋 ——
          看起來開著實際上關著。要放寬就調高次數，不要把視窗設成 0。
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
              description="清掃前最多追蹤幾個 key，六組共用。這是記憶體上限不是安全設定：每個 key 都是未認證陌生人自己選的字串，這個數字決定他們能讓 process 佔多少記憶體。上限 100000。"
              field={field}
              fieldState={fieldState}
              id="throttle-max-tracked-keys"
              label="追蹤 key 數上限"
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
  const copy = COUNTER_COPY[counter];

  return (
    <FieldSet>
      <FieldLegend variant="label">
        {copy.title}
        <span className="ml-2 font-normal text-muted-foreground">
          依 {copy.keyedOn} 計數
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
              label="允許次數"
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
              label="視窗長度（秒）"
              min={1}
            />
          )}
        />
      </div>
    </FieldSet>
  );
}
