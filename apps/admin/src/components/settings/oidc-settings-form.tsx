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
import { Switch } from "@tessera/ui/components/switch";
import { Controller, useForm } from "react-hook-form";

import { oidcSettingsSchema } from "@/lib/settings-schema";
import type { OidcSettingsValues } from "@/lib/settings-schema";

const DEFAULT_VALUES: OidcSettingsValues = {
  clientId: "",
  clientSecret: "",
  enabled: false,
  issuerUrl: "",
  scopes: "openid profile email",
};

export function OidcSettingsForm() {
  const { control, handleSubmit, watch } = useForm<OidcSettingsValues>({
    defaultValues: DEFAULT_VALUES,
    resolver: zodResolver(oidcSettingsSchema),
  });

  const enabled = watch("enabled");

  const onSubmit = handleSubmit((_values) => {
    // 待補：串接後端 API（server 尚無 OIDC 設定端點）
  });

  return (
    <form onSubmit={onSubmit}>
      <FieldGroup>
        <Controller
          control={control}
          name="enabled"
          render={({ field }) => (
            <Field orientation="horizontal">
              <FieldLabel htmlFor="oidc-enabled">
                啟用 OIDC 登入
                <FieldDescription>
                  開啟後訪客可透過外部身分提供者登入。此設定尚待後端支援。
                </FieldDescription>
              </FieldLabel>
              <Switch
                checked={field.value}
                id="oidc-enabled"
                onCheckedChange={field.onChange}
              />
            </Field>
          )}
        />
        <Controller
          control={control}
          name="issuerUrl"
          render={({ field, fieldState }) => (
            <Field data-invalid={fieldState.invalid}>
              <FieldLabel htmlFor="oidc-issuer-url">Issuer URL</FieldLabel>
              <Input
                {...field}
                aria-invalid={fieldState.invalid}
                disabled={!enabled}
                id="oidc-issuer-url"
                placeholder="https://accounts.example.com"
              />
              <FieldDescription>
                身分提供者的 OpenID Connect discovery 位址。
              </FieldDescription>
              {fieldState.invalid && <FieldError errors={[fieldState.error]} />}
            </Field>
          )}
        />
        <Controller
          control={control}
          name="clientId"
          render={({ field, fieldState }) => (
            <Field data-invalid={fieldState.invalid}>
              <FieldLabel htmlFor="oidc-client-id">Client ID</FieldLabel>
              <Input
                {...field}
                aria-invalid={fieldState.invalid}
                disabled={!enabled}
                id="oidc-client-id"
              />
              {fieldState.invalid && <FieldError errors={[fieldState.error]} />}
            </Field>
          )}
        />
        <Controller
          control={control}
          name="clientSecret"
          render={({ field, fieldState }) => (
            <Field data-invalid={fieldState.invalid}>
              <FieldLabel htmlFor="oidc-client-secret">
                Client Secret
              </FieldLabel>
              <Input
                {...field}
                aria-invalid={fieldState.invalid}
                disabled={!enabled}
                id="oidc-client-secret"
                type="password"
              />
              {fieldState.invalid && <FieldError errors={[fieldState.error]} />}
            </Field>
          )}
        />
        <Controller
          control={control}
          name="scopes"
          render={({ field, fieldState }) => (
            <Field data-invalid={fieldState.invalid}>
              <FieldLabel htmlFor="oidc-scopes">Scopes</FieldLabel>
              <Input
                {...field}
                aria-invalid={fieldState.invalid}
                disabled={!enabled}
                id="oidc-scopes"
                placeholder="openid profile email"
              />
              <FieldDescription>以空白分隔多個 scope。</FieldDescription>
              {fieldState.invalid && <FieldError errors={[fieldState.error]} />}
            </Field>
          )}
        />
        <Field orientation="horizontal">
          <Button type="submit">儲存</Button>
        </Field>
      </FieldGroup>
    </form>
  );
}
