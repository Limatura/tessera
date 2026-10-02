import { zodResolver } from "@hookform/resolvers/zod";
import { Button } from "@tessera/ui/components/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import { cn } from "@tessera/ui/lib/utils";
import { motion } from "motion/react";
import { Controller, useForm } from "react-hook-form";

import type { PasswordResetRequestError } from "@/hooks/use-auth";
import { useRequestPasswordReset } from "@/hooks/use-auth";
import { useCapabilities } from "@/hooks/use-capabilities";
import { apiErrorMessage } from "@/lib/api-error";
import type { ForgotPasswordFormValues } from "@/lib/api/auth-schema";
import { forgotPasswordSchema } from "@/lib/api/auth-schema";
import { containerVariants, itemVariants } from "@/lib/motion";
import * as m from "@/paraglide/messages";

const SECONDS_PER_MINUTE = 60;

function errorDescription(error: PasswordResetRequestError): string {
  switch (error.code) {
    case "too_many_requests": {
      const minutes = Math.max(
        1,
        Math.ceil(
          (error.retryAfterSeconds ?? SECONDS_PER_MINUTE) / SECONDS_PER_MINUTE
        )
      );
      return m.auth_reset_request_throttled({ minutes });
    }
    // 404：這台沒設寄信，理論上 capabilities 已經把連結藏起來，仍防一下舊快取。
    case "not_found": {
      return m.auth_reset_request_unavailable();
    }
    default: {
      return apiErrorMessage(error, m.common_try_again());
    }
  }
}

export function ForgotPasswordForm({
  className,
  ...props
}: React.ComponentProps<"div">) {
  const { data: capabilities } = useCapabilities();
  const requestReset = useRequestPasswordReset();
  const { control, handleSubmit } = useForm<ForgotPasswordFormValues>({
    defaultValues: { email: "" },
    resolver: zodResolver(forgotPasswordSchema),
  });

  const onSubmit = handleSubmit((values) => {
    requestReset.mutate(values);
  });

  const formError = requestReset.isError
    ? errorDescription(requestReset.error as PasswordResetRequestError)
    : null;

  return (
    <div className={cn("flex flex-col gap-6", className)} {...props}>
      <motion.div
        animate="visible"
        initial="hidden"
        variants={containerVariants}
      >
        <motion.div variants={itemVariants}>
          <Card className="border-border/60 bg-card/60 shadow-2xl shadow-black/5 backdrop-blur-2xl">
            <CardHeader>
              <motion.div variants={itemVariants}>
                <CardTitle className="font-medium text-xl tracking-tight">
                  {m.auth_forgot_password_title()}
                </CardTitle>
              </motion.div>
              <motion.div variants={itemVariants}>
                <CardDescription>
                  {requestReset.isSuccess
                    ? m.auth_reset_link_sent_description()
                    : m.auth_forgot_password_description()}
                </CardDescription>
              </motion.div>
            </CardHeader>
            {requestReset.isSuccess ? null : (
              <CardContent>
                <form onSubmit={onSubmit}>
                  <FieldGroup>
                    <motion.div variants={itemVariants}>
                      <Controller
                        control={control}
                        name="email"
                        render={({ field, fieldState }) => (
                          <Field data-invalid={fieldState.invalid}>
                            <FieldLabel htmlFor="email">
                              {m.auth_email()}
                            </FieldLabel>
                            <Input
                              {...field}
                              aria-invalid={fieldState.invalid}
                              id="email"
                              placeholder="m@example.com"
                              type="email"
                            />
                            {fieldState.invalid && (
                              <FieldError errors={[fieldState.error]} />
                            )}
                          </Field>
                        )}
                      />
                    </motion.div>
                    <motion.div variants={itemVariants}>
                      <Field>
                        {formError && <FieldError>{formError}</FieldError>}
                        <motion.div whileTap={{ scale: 0.98 }}>
                          <Button
                            className="w-full"
                            disabled={
                              requestReset.isPending ||
                              capabilities?.password_reset === false
                            }
                            type="submit"
                          >
                            {requestReset.isPending
                              ? m.auth_sending_reset_link()
                              : m.auth_send_reset_link()}
                          </Button>
                        </motion.div>
                        <FieldDescription className="text-center">
                          <a
                            className="underline-offset-4 hover:underline"
                            href="/login"
                          >
                            {m.auth_back_to_login()}
                          </a>
                        </FieldDescription>
                      </Field>
                    </motion.div>
                  </FieldGroup>
                </form>
              </CardContent>
            )}
          </Card>
        </motion.div>
      </motion.div>
    </div>
  );
}
