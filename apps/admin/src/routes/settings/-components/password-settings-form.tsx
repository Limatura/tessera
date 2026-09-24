import { zodResolver } from "@hookform/resolvers/zod";
import type { components } from "@tessera/api/schema";
import { FieldGroup, FieldSeparator } from "@tessera/ui/components/field";
import { Controller } from "react-hook-form";

import { readSetting } from "@/hooks/use-settings";
import { useSettingsForm } from "@/hooks/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/use-settings-form";
import {
  PASSWORD_MAX_CEILING,
  PASSWORD_MIN_FLOOR,
  passwordSettingsSchema,
} from "@/lib/settings-schema";
import type { PasswordSettingsValues } from "@/lib/settings-schema";
import { asBoolean, asNumber, asString } from "@/lib/settings-values";

import { SwitchField, TextField } from "./form-fields";
import { NumberField } from "./number-field";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";

type Settings = components["schemas"]["SettingsResponse"];

const DEFAULT_MIN_LENGTH = 8;
const DEFAULT_MAX_LENGTH = 256;
const DEFAULT_MEMORY_KIB = 19_456;
const DEFAULT_ITERATIONS = 2;
const DEFAULT_LANES = 1;
const DEFAULT_PWNED_URL = "https://api.pwnedpasswords.com";

const KEYS = {
  iterations: "auth.hashing.iterations",
  lanes: "auth.hashing.lanes",
  maxLength: "auth.password.max_length",
  memoryKib: "auth.hashing.memory_kib",
  minLength: "auth.password.min_length",
  pwnedEnabled: "auth.pwned.enabled",
  pwnedUrl: "auth.pwned.url",
  requireDigit: "auth.password.require_digit",
  requireMixedCase: "auth.password.require_mixed_case",
  requireSymbol: "auth.password.require_symbol",
} satisfies SettingsKeyMap<PasswordSettingsValues>;

export function PasswordSettingsForm() {
  return (
    <SettingsLoader>
      {(settings) => (
        <PasswordSettingsFields defaultValues={initialValues(settings)} />
      )}
    </SettingsLoader>
  );
}

function initialValues(settings: Settings): PasswordSettingsValues {
  return {
    iterations: asNumber(
      readSetting(settings, KEYS.iterations),
      DEFAULT_ITERATIONS
    ),
    lanes: asNumber(readSetting(settings, KEYS.lanes), DEFAULT_LANES),
    maxLength: asNumber(
      readSetting(settings, KEYS.maxLength),
      DEFAULT_MAX_LENGTH
    ),
    memoryKib: asNumber(
      readSetting(settings, KEYS.memoryKib),
      DEFAULT_MEMORY_KIB
    ),
    minLength: asNumber(
      readSetting(settings, KEYS.minLength),
      DEFAULT_MIN_LENGTH
    ),
    pwnedEnabled: asBoolean(readSetting(settings, KEYS.pwnedEnabled), false),
    pwnedUrl: asString(readSetting(settings, KEYS.pwnedUrl), DEFAULT_PWNED_URL),
    requireDigit: asBoolean(readSetting(settings, KEYS.requireDigit), false),
    requireMixedCase: asBoolean(
      readSetting(settings, KEYS.requireMixedCase),
      false
    ),
    requireSymbol: asBoolean(readSetting(settings, KEYS.requireSymbol), false),
  };
}

function PasswordSettingsFields({
  defaultValues,
}: {
  defaultValues: PasswordSettingsValues;
}) {
  const form = useSettingsForm<PasswordSettingsValues>({
    defaultValues,
    keys: KEYS,
    resolver: zodResolver(passwordSettingsSchema),
  });

  return (
    <form onSubmit={form.submit}>
      <FieldGroup>
        <p className="text-muted-foreground text-sm">
          長度是主要規則。依 NIST 800-63B：組成規則只會逼出{" "}
          <code>Password1!</code>
          ，長度比字元集更有價值，所以下面三個預設都是關的。留著它們是為了還在要求組成規則的內部政策。
        </p>

        <Controller
          control={form.control}
          name="minLength"
          render={({ field, fieldState }) => (
            <NumberField
              description="下限 8，不能再低。"
              field={field}
              fieldState={fieldState}
              id="password-min-length"
              label="最短長度"
              max={PASSWORD_MAX_CEILING}
              min={PASSWORD_MIN_FLOOR}
            />
          )}
        />

        <Controller
          control={form.control}
          name="maxLength"
          render={({ field, fieldState }) => (
            <NumberField
              description="上限 1024。需要上限是因為無上限的密碼欄位等於「未認證請求可叫伺服器做任意多的工」。"
              field={field}
              fieldState={fieldState}
              id="password-max-length"
              label="最長長度"
              max={PASSWORD_MAX_CEILING}
              min={PASSWORD_MIN_FLOOR}
            />
          )}
        />

        <FieldSeparator />

        <p className="text-muted-foreground text-sm">
          組成規則只管
          <strong className="font-medium text-foreground">選新密碼</strong>
          （註冊、改密碼、重設）。登入不檢查，所以開起來
          <strong className="font-medium text-foreground">
            不會把既有帳號鎖在外面
          </strong>
          ，但他們下次改密碼時就得照新規則。字元類別照 Unicode
          判斷，任何語言的密碼都用同一套標準。
        </p>

        <Controller
          control={form.control}
          name="requireMixedCase"
          render={({ field }) => (
            <SwitchField
              description="要同時有大寫和小寫字母。"
              field={field}
              id="password-require-mixed-case"
              label="需要大小寫混合"
            />
          )}
        />

        <Controller
          control={form.control}
          name="requireDigit"
          render={({ field }) => (
            <SwitchField
              description="要有數字。"
              field={field}
              id="password-require-digit"
              label="需要數字"
            />
          )}
        />

        <Controller
          control={form.control}
          name="requireSymbol"
          render={({ field }) => (
            <SwitchField
              description="要有符號。空白不算 —— 不然任何 passphrase 都自動過關，等於沒開。"
              field={field}
              id="password-require-symbol"
              label="需要符號"
            />
          )}
        />

        <FieldSeparator />

        <p className="text-muted-foreground text-sm">
          外洩密碼檢查一樣只管選新密碼。送出去的只有密碼 SHA-1 的前 5
          個字，比對在本機做（k-anonymity），完整密碼和完整雜湊都不會離開這台。
          查不到服務（逾時、連不上、非 200）就
          <strong className="font-medium text-foreground">放行</strong>
          ，只在 log 留一行 —— 註冊和重設密碼不能因為別人的服務掛掉而停擺。
        </p>

        <Controller
          control={form.control}
          name="pwnedEnabled"
          render={({ field }) => (
            <SwitchField
              description="設新密碼時查 Have I Been Pwned，出現在外洩名單上的密碼會被拒絕。這台要連得到下面那個位址。"
              field={field}
              id="pwned-enabled"
              label="檢查外洩密碼"
            />
          )}
        />

        <Controller
          control={form.control}
          name="pwnedUrl"
          render={({ field, fieldState }) => (
            <TextField
              description="range API 的位址，預設官方服務。不想依賴它、或這台出不了網路的話，可以指到自架的實作。"
              field={field}
              fieldState={fieldState}
              id="pwned-url"
              label="服務位址"
              placeholder={DEFAULT_PWNED_URL}
            />
          )}
        />

        <FieldSeparator />

        <p className="text-muted-foreground text-sm">
          以下是 Argon2id 的成本參數。改這些
          <strong className="font-medium text-foreground">
            不會讓既有密碼失效
          </strong>
          ：參數寫在雜湊值本身裡，驗證時從那讀，所以只影響新產生的雜湊。
        </p>

        <Controller
          control={form.control}
          name="memoryKib"
          render={({ field, fieldState }) => (
            <NumberField
              description="預設 19456（19 MiB，OWASP 建議值）。最關鍵的參數 —— 記憶體成本是 GPU 破解變貴的原因，但也等於登入尖峰的記憶體用量乘以同時執行數，所以調高前先算一下這台機器撐不撐得住。"
              field={field}
              fieldState={fieldState}
              id="hashing-memory-kib"
              label="記憶體成本（KiB）"
              min={8}
            />
          )}
        />

        <Controller
          control={form.control}
          name="iterations"
          render={({ field, fieldState }) => (
            <NumberField
              description="預設 2。"
              field={field}
              fieldState={fieldState}
              id="hashing-iterations"
              label="迭代次數"
              min={1}
            />
          )}
        />

        <Controller
          control={form.control}
          name="lanes"
          render={({ field, fieldState }) => (
            <NumberField
              description="預設 1。"
              field={field}
              fieldState={fieldState}
              id="hashing-lanes"
              label="平行度（lanes）"
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
