import { zodResolver } from "@hookform/resolvers/zod";
import { IconAlertTriangle } from "@tabler/icons-react";
import type { components } from "@tessera/api/schema";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  FieldSeparator,
} from "@tessera/ui/components/field";
import { Textarea } from "@tessera/ui/components/textarea";
import { Controller, useWatch } from "react-hook-form";

import { readSetting } from "@/hooks/use-settings";
import { useSettingsForm } from "@/hooks/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/use-settings-form";
import { featureSettingsSchema } from "@/lib/settings-schema";
import type { FeatureSettingsValues } from "@/lib/settings-schema";
import { asBoolean, asString } from "@/lib/settings-values";

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

  return (
    <form onSubmit={form.submit}>
      <FieldGroup>
        <p className="text-muted-foreground text-sm">
          執行期開關。關掉的功能路由根本不會掛上，所以呼叫它得到單純的
          404，而不是一個 等於確認「它存在」的 403。
        </p>

        {!admin && <AdminLockoutWarning />}

        <Controller
          control={form.control}
          name="admin"
          render={({ field }) => (
            <SwitchField
              description="這個後台整個依賴的端點。關掉就再也打不開這個畫面，只能改環境變數 TESSERA_FEATURES__ADMIN=true 後重啟才回得來。"
              field={field}
              id="features-admin"
              label="管理端點"
            />
          )}
        />

        <Controller
          control={form.control}
          name="accounts"
          render={({ field }) => (
            <SwitchField
              description="註冊、登入與 session。關掉等於完全沒有帳號，適合「一個人在 VPN 後面只要一個檔案投放點」。與匿名上傳刻意獨立 —— 想要管理面板不等於想強迫所有人註冊。"
              field={field}
              id="features-accounts"
              label="帳號功能"
            />
          )}
        />

        {!accounts && admin && (
          <p className="rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm">
            關掉帳號功能後就無法登入，而這個後台要 session 才進得來 ——
            下次重新載入就會被擋在外面。
          </p>
        )}

        <FieldSeparator />

        <Controller
          control={form.control}
          name="ogLayout"
          render={({ field, fieldState }) => (
            <Field data-invalid={fieldState.invalid}>
              <FieldLabel htmlFor="features-og-layout">
                自訂預覽卡片 HTML
              </FieldLabel>
              <Textarea
                {...field}
                aria-invalid={fieldState.invalid}
                className="font-mono text-xs"
                id="features-og-layout"
                rows={6}
              />
              <FieldDescription>
                留空使用內建卡片。設了就取代所有分享的卡片。只允許{" "}
                <code>div</code>、<code>span</code>、<code>style</code>
                ，且不能指向本伺服器以外的資源，違反的話後端直接拒絕。鎖定分享的卡片裡{" "}
                <code>{"{{size}}"}</code> 會是空的 ——
                因為值是空的，不是因為樣板有做防護。
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
 */
function AdminLockoutWarning() {
  return (
    <div
      className="flex gap-3 rounded-md border-2 border-destructive/50 bg-destructive/10 p-3 text-sm"
      role="alert"
    >
      <IconAlertTriangle className="mt-0.5 size-4 shrink-0 text-destructive" />
      <div className="space-y-1">
        <p className="font-medium">存檔後你會失去這個後台</p>
        <p className="text-muted-foreground">
          管理端點關掉後，這個畫面依賴的 API 全部回
          404，面板自己改不回來。唯一的復原方式 是在伺服器上設{" "}
          <code>TESSERA_FEATURES__ADMIN=true</code> 然後重啟。確定要這樣做
          再按儲存。
        </p>
      </div>
    </div>
  );
}
