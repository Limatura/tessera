import { zodResolver } from "@hookform/resolvers/zod";
import { Button } from "@tessera/ui/components/button";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import { Controller, useForm } from "react-hook-form";

import { storageSettingsSchema } from "@/lib/settings-schema";
import type { StorageSettingsValues } from "@/lib/settings-schema";

import { NumberField } from "./number-field";

const DEFAULT_PART_SIZE_BYTES = 8 * 1024 * 1024;

const DEFAULT_VALUES: StorageSettingsValues = {
  backendUrl: "data/blobs",
  partSizeBytes: DEFAULT_PART_SIZE_BYTES,
};

export function StorageSettingsForm() {
  const { control, handleSubmit } = useForm<StorageSettingsValues>({
    defaultValues: DEFAULT_VALUES,
    resolver: zodResolver(storageSettingsSchema),
  });

  const onSubmit = handleSubmit((_values) => {
    // 待補：串接後端 API（對應 storage.url / server.part_size_bytes）
  });

  return (
    <form onSubmit={onSubmit}>
      <FieldGroup>
        <Controller
          control={control}
          name="backendUrl"
          render={({ field, fieldState }) => (
            <Field data-invalid={fieldState.invalid}>
              <FieldLabel htmlFor="storage-backend-url">
                儲存後端位址
              </FieldLabel>
              <Input
                {...field}
                aria-invalid={fieldState.invalid}
                id="storage-backend-url"
                placeholder="data/blobs 或 s3://bucket/prefix"
              />
              <FieldDescription>
                本機路徑會建立本地資料夾；帶協定的位址（s3://、gs://、az://）交由
                object_store 處理，對應 config.rs 的 storage.url。
              </FieldDescription>
              {fieldState.invalid && <FieldError errors={[fieldState.error]} />}
            </Field>
          )}
        />
        <Controller
          control={control}
          name="partSizeBytes"
          render={({ field, fieldState }) => (
            <NumberField
              description="累積多少資料才送出一個分片，S3 相容後端至少需要 5 MiB；使用 Cloudflare R2 時建議與最小分片大小一致。"
              field={field}
              fieldState={fieldState}
              id="storage-part-size"
              label="分片上傳大小（bytes）"
              min={5 * 1024 * 1024}
            />
          )}
        />
        <Field orientation="horizontal">
          <Button type="submit">儲存</Button>
        </Field>
      </FieldGroup>
    </form>
  );
}
