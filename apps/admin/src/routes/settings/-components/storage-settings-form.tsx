import { zodResolver } from "@hookform/resolvers/zod";
import type { components } from "@tessera/api/schema";
import { FieldGroup, FieldSeparator } from "@tessera/ui/components/field";
import { Controller } from "react-hook-form";

import { readSetting } from "@/hooks/use-settings";
import { useSettingsForm } from "@/hooks/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/use-settings-form";
import {
  MIN_PART_SIZE_BYTES,
  storageSettingsSchema,
} from "@/lib/settings-schema";
import type { StorageSettingsValues } from "@/lib/settings-schema";
import { asNumber } from "@/lib/settings-values";

import { NumberField } from "./number-field";
import { ReadOnlySetting } from "./read-only-setting";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";

type Settings = components["schemas"]["SettingsResponse"];

const DEFAULT_PART_SIZE_BYTES = 8 * 1024 * 1024;

const KEYS = {
  partSizeBytes: "server.part_size_bytes",
} satisfies SettingsKeyMap<StorageSettingsValues>;

export function StorageSettingsForm() {
  return (
    <SettingsLoader>
      {(settings) => (
        <StorageSettingsFields defaultValues={initialValues(settings)} />
      )}
    </SettingsLoader>
  );
}

function initialValues(settings: Settings): StorageSettingsValues {
  return {
    partSizeBytes: asNumber(
      readSetting(settings, KEYS.partSizeBytes),
      DEFAULT_PART_SIZE_BYTES
    ),
  };
}

function StorageSettingsFields({
  defaultValues,
}: {
  defaultValues: StorageSettingsValues;
}) {
  const form = useSettingsForm<StorageSettingsValues>({
    defaultValues,
    keys: KEYS,
    resolver: zodResolver(storageSettingsSchema),
  });

  return (
    <form onSubmit={form.submit}>
      <FieldGroup>
        <ReadOnlySetting
          description={
            <>
              blob 存在哪。URL 裡帶帳密（<code>s3://key:secret@…</code>
              ），所以面板既讀不到也寫不了 —— 只能用環境變數{" "}
              <code>TESSERA_STORAGE__URL</code> 設定，對應 storage.url。
            </>
          }
          label="儲存後端位址"
          reason="只能用環境變數設定"
          value={null}
        />

        <FieldSeparator />

        <Controller
          control={form.control}
          name="partSizeBytes"
          render={({ field, fieldState }) => (
            <NumberField
              description="累積多少資料才送出一個分片。S3 拒絕最後一片以外小於 5 MiB 的 part；Cloudflare R2 還要求每片大小相同，所以在 R2 上這個值應該就等於最小值。"
              field={field}
              fieldState={fieldState}
              id="storage-part-size"
              label="分片上傳大小（bytes）"
              min={MIN_PART_SIZE_BYTES}
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
