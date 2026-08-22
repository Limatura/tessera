import { zodResolver } from "@hookform/resolvers/zod";
import { Button } from "@tessera/ui/components/button";
import { Field, FieldGroup } from "@tessera/ui/components/field";
import { Controller, useForm } from "react-hook-form";

import { sessionSettingsSchema } from "@/lib/settings-schema";
import type { SessionSettingsValues } from "@/lib/settings-schema";

import { NumberField } from "./number-field";

const DEFAULT_SESSION_TTL_HOURS = 24 * 30;

const DEFAULT_VALUES: SessionSettingsValues = {
  sessionTtlHours: DEFAULT_SESSION_TTL_HOURS,
};

export function SessionSettingsForm() {
  const { control, handleSubmit } = useForm<SessionSettingsValues>({
    defaultValues: DEFAULT_VALUES,
    resolver: zodResolver(sessionSettingsSchema),
  });

  const onSubmit = handleSubmit((_values) => {
    // 待補：串接後端 API（對應 auth.session_ttl_hours）
  });

  return (
    <form onSubmit={onSubmit}>
      <FieldGroup>
        <Controller
          control={control}
          name="sessionTtlHours"
          render={({ field, fieldState }) => (
            <NumberField
              description="使用者登入後多久需要重新登入，預設 720 小時（30 天）。"
              field={field}
              fieldState={fieldState}
              id="session-ttl-hours"
              label="登入會話時長（小時）"
              min={1}
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
