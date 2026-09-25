import { zodResolver } from "@hookform/resolvers/zod";
import { useNavigate } from "@tanstack/react-router";
import type { components } from "@tessera/api/schema";
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
import { toast } from "@tessera/ui/components/toast";
import { cn } from "@tessera/ui/lib/utils";
import { motion } from "motion/react";
import { useMemo } from "react";
import { Controller, useForm, useWatch } from "react-hook-form";
import { z } from "zod";

import { PasswordRequirements } from "@/components/password-requirements";
import { usePasswordReset } from "@/hooks/use-auth";
import { usePasswordBreach } from "@/hooks/use-password-breach";
import { usePasswordPolicy } from "@/hooks/use-password-policy";
import {
  isBreachedPasswordError,
  passwordSchemaFor,
} from "@/lib/api/password-policy";
import { containerVariants, itemVariants } from "@/lib/motion";
import * as m from "@/paraglide/messages";

interface ResetFormValues {
  confirmPassword: string;
  password: string;
}

export function PasswordResetForm({
  className,
  ...props
}: React.ComponentProps<"div">) {
  const navigate = useNavigate();
  const policy = usePasswordPolicy();
  const resetMutation = usePasswordReset();

  const tokenFromUrl = useMemo(() => {
    if (typeof window === "undefined") {
      return null;
    }
    const token = new URLSearchParams(window.location.search).get("token");
    window.history.replaceState({}, "", "/reset-password");
    return token;
  }, []);

  const schema = useMemo(
    () =>
      z
        .object({
          confirmPassword: z.string(),
          password: passwordSchemaFor(policy),
        })
        .refine((data) => data.password === data.confirmPassword, {
          message: m.auth_password_mismatch(),
          path: ["confirmPassword"],
        }),
    [policy]
  );

  const { control, handleSubmit, setError } = useForm<ResetFormValues>({
    defaultValues: { confirmPassword: "", password: "" },
    resolver: zodResolver(schema),
  });
  const password = useWatch({ control, name: "password" });
  const breached = usePasswordBreach(password, policy);

  const onSubmit = handleSubmit((values) => {
    // Already shown under the field; the server would only say it again.
    if (breached) {
      return;
    }
    if (!tokenFromUrl) {
      toast.add({
        description: "Reset link is missing",
        title: m.auth_error(),
        type: "error",
      });
      return;
    }

    resetMutation.mutate(
      { password: values.password, token: tokenFromUrl },
      {
        onError: (error) => {
          // Marked on the field like any other refused password: the token
          // is still good, so choosing another one and resubmitting works.
          if (isBreachedPasswordError(error)) {
            setError("password", {
              message: m.auth_password_breached(),
              type: "server",
            });
            return;
          }
          const errorBody =
            error instanceof Error && "code" in error && "message" in error
              ? (error as components["schemas"]["ErrorBody"])
              : null;
          toast.add({
            description: errorBody?.message ?? "Failed to reset password",
            title: m.auth_error(),
            type: "error",
          });
        },
        onSuccess: () => {
          toast.add({
            description: m.auth_password_reset_success(),
            title: m.auth_success(),
            type: "success",
          });
          navigate({ to: "/login" });
        },
      }
    );
  });

  if (!tokenFromUrl) {
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
                    {m.auth_error()}
                  </CardTitle>
                </motion.div>
                <motion.div variants={itemVariants}>
                  <CardDescription>
                    {m.auth_password_reset_invalid_link()}
                  </CardDescription>
                </motion.div>
              </CardHeader>
            </Card>
          </motion.div>
        </motion.div>
      </div>
    );
  }

  const formError = isBreachedPasswordError(resetMutation.error)
    ? null
    : (resetMutation.error as components["schemas"]["ErrorBody"] | null);

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
              <motion.div
                className="mb-2 flex items-center gap-2"
                variants={itemVariants}
              >
                <span className="relative flex h-1.5 w-1.5">
                  <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-primary/60" />
                  <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-primary" />
                </span>
                <span className="text-muted-foreground text-xs">
                  {m.status_receiving_open()}
                </span>
              </motion.div>
              <motion.div variants={itemVariants}>
                <CardTitle className="font-medium text-xl tracking-tight">
                  {m.auth_reset_password()}
                </CardTitle>
              </motion.div>
              <motion.div variants={itemVariants}>
                <CardDescription>{m.auth_enter_new_password()}</CardDescription>
              </motion.div>
            </CardHeader>
            <CardContent>
              <form onSubmit={onSubmit}>
                <FieldGroup>
                  <motion.div variants={itemVariants}>
                    <Controller
                      control={control}
                      name="password"
                      render={({ field, fieldState }) => (
                        <Field data-invalid={fieldState.invalid}>
                          <FieldLabel htmlFor="password">
                            {m.auth_password()}
                          </FieldLabel>
                          <Input
                            {...field}
                            aria-invalid={fieldState.invalid}
                            id="password"
                            placeholder={m.auth_enter_password()}
                            type="password"
                          />
                          <PasswordRequirements
                            breached={breached}
                            invalid={fieldState.invalid}
                            policy={policy}
                            value={field.value}
                          />
                          {fieldState.error?.type === "server" && !breached && (
                            <FieldError errors={[fieldState.error]} />
                          )}
                        </Field>
                      )}
                    />
                  </motion.div>
                  <motion.div variants={itemVariants}>
                    <Controller
                      control={control}
                      name="confirmPassword"
                      render={({ field, fieldState }) => (
                        <Field data-invalid={fieldState.invalid}>
                          <FieldLabel htmlFor="confirmPassword">
                            {m.auth_confirm_password()}
                          </FieldLabel>
                          <Input
                            {...field}
                            aria-invalid={fieldState.invalid}
                            id="confirmPassword"
                            placeholder={m.auth_confirm_password()}
                            type="password"
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
                      {formError && (
                        <FieldError>{formError.message}</FieldError>
                      )}
                      <motion.div whileTap={{ scale: 0.98 }}>
                        <Button
                          className="w-full"
                          disabled={resetMutation.isPending}
                          type="submit"
                        >
                          {resetMutation.isPending
                            ? m.auth_resetting()
                            : m.auth_reset_password()}
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
          </Card>
        </motion.div>
      </motion.div>
    </div>
  );
}
