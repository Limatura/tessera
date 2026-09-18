import type { components } from "@tessera/api/schema";
import type { DefaultValues, FieldValues, UseFormProps } from "react-hook-form";
import { useForm } from "react-hook-form";

import { useSession } from "@/hooks/use-auth";
import { useUpdateSettings } from "@/hooks/use-settings";
import { PERMISSIONS, hasPermission } from "@/lib/permissions";

type ErrorBody = components["schemas"]["ErrorBody"];

/** 表單欄位名 → server 的 dotted key。省略的欄位永遠不會被送出。 */
export type SettingsKeyMap<TValues extends FieldValues> = Partial<
  Record<keyof TValues & string, string>
>;

/**
 * 送出前的轉換。回 `undefined` 代表「這次不要送這個 key」—— SEALED 欄位留空就是
 * 走這條，使用者沒填的 secret 不該被一個空字串覆蓋掉。
 */
export type SettingsEncoders<TValues extends FieldValues> = Partial<{
  [K in keyof TValues]: (value: TValues[K]) => unknown;
}>;

/**
 * 只挑出真正改動過的欄位。
 *
 * `PUT /admin/settings` 把 changes 當一組驗證，一個 key 不合法會讓整批退回 ——
 * 所以沒動過的欄位就算值是對的也別送。
 */
export function collectChanges<TValues extends FieldValues>(
  keys: SettingsKeyMap<TValues>,
  dirtyFields: Record<string, unknown>,
  values: TValues,
  encoders?: SettingsEncoders<TValues>
): Record<string, unknown> {
  const changes: Record<string, unknown> = {};

  for (const name of Object.keys(keys) as (keyof TValues & string)[]) {
    const key = keys[name];
    if (!(key && dirtyFields[name])) {
      continue;
    }

    const encode = encoders?.[name];
    const encoded = encode ? encode(values[name]) : values[name];
    if (encoded === undefined) {
      continue;
    }

    changes[key] = encoded;
  }

  return changes;
}

/** `PUT /admin/settings` 失敗時 server 給的說明。 */
export function settingsErrorMessage(error: unknown): string | null {
  if (!error) {
    return null;
  }
  const body = error as ErrorBody;
  return body.message ?? "儲存失敗";
}

/**
 * 每個設定表單共用的那一半：react-hook-form、寫入權限、以及「只送 dirty 欄位」的
 * 送出流程。
 *
 * 初始值由呼叫端從 `GET /admin/settings` 讀好再傳進來，而且只在掛載時讀一次 ——
 * 背景重抓不該把使用者打到一半的內容洗掉。
 */
export function useSettingsForm<TValues extends FieldValues>({
  defaultValues,
  resolver,
  keys,
  encoders,
}: {
  defaultValues: TValues;
  resolver: UseFormProps<TValues>["resolver"];
  keys: SettingsKeyMap<TValues>;
  encoders?: SettingsEncoders<TValues>;
}) {
  const { data: account } = useSession();
  const update = useUpdateSettings();
  const form = useForm<TValues>({
    defaultValues: defaultValues as DefaultValues<TValues>,
    resolver,
  });

  // 在 render 期間讀出來，react-hook-form 才會訂閱這兩個 —— 只在送出的 callback
  // 裡碰 `formState.dirtyFields` 的話它根本不會被計算。
  const { dirtyFields, isDirty } = form.formState;

  const submit = form.handleSubmit((values) => {
    const changes = collectChanges(
      keys,
      dirtyFields as Record<string, unknown>,
      values,
      encoders
    );

    // 只動了 SEALED 欄位又把它清空的話，這裡會是空的。reset 讓「已儲存」的狀態
    // 回到乾淨，而不是留一顆按不動的按鈕。
    if (Object.keys(changes).length === 0) {
      form.reset(values);
      return;
    }

    update.mutate(changes, { onSuccess: () => form.reset(values) });
  });

  return {
    canWrite: hasPermission(account, PERMISSIONS.settingsWrite),
    control: form.control,
    errorMessage: settingsErrorMessage(update.error),
    isDirty,
    isPending: update.isPending,
    isSaved: update.isSuccess && !isDirty,
    submit,
  };
}
