import { zodResolver } from "@hookform/resolvers/zod";
import { Button } from "@tessera/ui/components/button";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  FieldSeparator,
} from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import { Switch } from "@tessera/ui/components/switch";
import { Controller, useForm } from "react-hook-form";

import { uploadSettingsSchema } from "@/lib/settings-schema";
import type { UploadSettingsValues } from "@/lib/settings-schema";

import { GuestAnonymousWarning } from "./guest-anonymous-warning";
import { NumberField } from "./number-field";

const DEFAULT_MAX_BODY_BYTES = 100 * 1024 * 1024;
const DEFAULT_THROTTLE_WINDOW_SECS = 300;
const DEFAULT_THROTTLE_MAX_ATTEMPTS = 20;
const DEFAULT_DEFAULT_EXPIRY_HOURS = 24 * 7;
const DEFAULT_MAX_EXPIRY_HOURS = 24 * 30;

const DEFAULT_VALUES: UploadSettingsValues = {
  allowAnonymousUpload: true,
  allowedFileTypes: "",
  defaultExpiryHours: DEFAULT_DEFAULT_EXPIRY_HOURS,
  maxBodyBytes: DEFAULT_MAX_BODY_BYTES,
  maxExpiryHours: DEFAULT_MAX_EXPIRY_HOURS,
  maxTotalBytes: 0,
  throttleMaxAttempts: DEFAULT_THROTTLE_MAX_ATTEMPTS,
  throttleWindowSecs: DEFAULT_THROTTLE_WINDOW_SECS,
};

export function UploadSettingsForm() {
  const { control, handleSubmit } = useForm<UploadSettingsValues>({
    defaultValues: DEFAULT_VALUES,
    resolver: zodResolver(uploadSettingsSchema),
  });

  const onSubmit = handleSubmit((_values) => {
    // 待補：串接後端 API
    // 對應 auth.throttle.source / server.max_body_bytes / features.anonymous_upload
    // / storage.max_total_bytes / share.default_expiry_hours / share.max_expiry_hours
  });

  return (
    <form onSubmit={onSubmit}>
      <FieldGroup>
        <GuestAnonymousWarning fixAt="account" />

        <Controller
          control={control}
          name="throttleWindowSecs"
          render={({ field, fieldState }) => (
            <NumberField
              description="在此時間內限制單一來源的上傳次數。"
              field={field}
              fieldState={fieldState}
              id="upload-throttle-window"
              label="限流時間窗口（秒）"
              min={1}
            />
          )}
        />
        <Controller
          control={control}
          name="throttleMaxAttempts"
          render={({ field, fieldState }) => (
            <NumberField
              description="限流窗口內，同一來源最多可以上傳幾個文件。"
              field={field}
              fieldState={fieldState}
              id="upload-throttle-attempts"
              label="允許上傳文件數"
              min={1}
            />
          )}
        />
        <Controller
          control={control}
          name="maxBodyBytes"
          render={({ field, fieldState }) => (
            <NumberField
              field={field}
              fieldState={fieldState}
              id="upload-max-body-bytes"
              label="單文件大小上限（bytes）"
              min={1}
            />
          )}
        />
        <Controller
          control={control}
          name="allowedFileTypes"
          render={({ field, fieldState }) => (
            <Field data-invalid={fieldState.invalid}>
              <FieldLabel htmlFor="upload-allowed-types">
                允許上傳類型
              </FieldLabel>
              <Input
                {...field}
                aria-invalid={fieldState.invalid}
                id="upload-allowed-types"
                placeholder=".png, .jpg, application/pdf"
              />
              <FieldDescription>
                以逗號分隔副檔名或 MIME 類型，留空代表不限制。
              </FieldDescription>
              {fieldState.invalid && <FieldError errors={[fieldState.error]} />}
            </Field>
          )}
        />
        <Controller
          control={control}
          name="allowAnonymousUpload"
          render={({ field }) => (
            <Field orientation="horizontal">
              <FieldLabel htmlFor="upload-allow-anonymous">
                允許訪客上傳
                <FieldDescription>
                  關閉後僅登入帳號可以上傳，對應 features.anonymous_upload。
                </FieldDescription>
              </FieldLabel>
              <Switch
                checked={field.value}
                id="upload-allow-anonymous"
                onCheckedChange={field.onChange}
              />
            </Field>
          )}
        />

        <FieldSeparator />

        <Controller
          control={control}
          name="maxTotalBytes"
          render={({ field, fieldState }) => (
            <NumberField
              description="所有有效分享合計的儲存上限，0 代表不限制。"
              field={field}
              fieldState={fieldState}
              id="upload-max-total-bytes"
              label="總存儲容量上限（bytes）"
              min={0}
            />
          )}
        />
        <Controller
          control={control}
          name="defaultExpiryHours"
          render={({ field, fieldState }) => (
            <NumberField
              description="上傳時未指定保存期限的預設值，0 代表沿用下方的最長保存時間。"
              field={field}
              fieldState={fieldState}
              id="upload-default-expiry"
              label="文件預設保存時間（小時）"
              min={0}
            />
          )}
        />
        <Controller
          control={control}
          name="maxExpiryHours"
          render={({ field, fieldState }) => (
            <NumberField
              description="使用者可要求的最長保存期限，0 代表沒有上限。"
              field={field}
              fieldState={fieldState}
              id="upload-max-expiry"
              label="文件最長保存時間（小時）"
              min={0}
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
