import { zodResolver } from "@hookform/resolvers/zod";
import { Link, useNavigate } from "@tanstack/react-router";
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
import { cn } from "@tessera/ui/lib/utils";
import { motion } from "motion/react";
import { useMemo } from "react";
import { Controller, useForm } from "react-hook-form";

import { PasswordRequirements } from "@/components/password-requirements";
import { useRegister } from "@/hooks/use-auth";
import { usePasswordPolicy } from "@/hooks/use-password-policy";
import { registerSchemaFor } from "@/lib/api/auth-schema";
import type { RegisterFormValues } from "@/lib/api/auth-schema";
import { containerVariants, itemVariants } from "@/lib/motion";
import * as m from "@/paraglide/messages";

export function RegisterForm({
  className,
  ...props
}: React.ComponentProps<"div">) {
  const navigate = useNavigate();
  const registerAccount = useRegister();
  const policy = usePasswordPolicy();
  const schema = useMemo(() => registerSchemaFor(policy), [policy]);
  const { control, handleSubmit } = useForm<RegisterFormValues>({
    defaultValues: { email: "", password: "", username: "" },
    resolver: zodResolver(schema),
  });

  const onSubmit = handleSubmit((values) => {
    registerAccount.mutate(values, {
      onSuccess: (account) => {
        // `account` is undefined when the server sent a confirmation link
        // instead of creating one outright — nothing to sign in to yet.
        navigate({ to: account ? "/" : "/login" });
      },
    });
  });

  const formError = registerAccount.error as
    | components["schemas"]["ErrorBody"]
    | null;

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
                  {m.auth_create_account_title()}
                </CardTitle>
              </motion.div>
              <motion.div variants={itemVariants}>
                <CardDescription>
                  {m.auth_sign_up_description()}
                </CardDescription>
              </motion.div>
            </CardHeader>
            <CardContent>
              <form onSubmit={onSubmit}>
                <FieldGroup>
                  <motion.div variants={itemVariants}>
                    <Controller
                      control={control}
                      name="username"
                      render={({ field, fieldState }) => (
                        <Field data-invalid={fieldState.invalid}>
                          <FieldLabel htmlFor="username">
                            {m.auth_username()}
                          </FieldLabel>
                          <Input
                            {...field}
                            aria-invalid={fieldState.invalid}
                            autoComplete="username"
                            id="username"
                            placeholder="alice"
                          />
                          {fieldState.invalid && (
                            <FieldError errors={[fieldState.error]} />
                          )}
                        </Field>
                      )}
                    />
                  </motion.div>
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
                            autoComplete="email"
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
                            autoComplete="new-password"
                            id="password"
                            type="password"
                          />
                          <PasswordRequirements
                            invalid={fieldState.invalid}
                            policy={policy}
                            value={field.value}
                          />
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
                          disabled={registerAccount.isPending}
                          type="submit"
                        >
                          {registerAccount.isPending
                            ? m.auth_creating_account()
                            : m.auth_create_account()}
                        </Button>
                      </motion.div>
                      <FieldDescription className="text-center">
                        {m.auth_already_have_account()}{" "}
                        <Link
                          className="underline-offset-4 hover:underline"
                          to="/login"
                        >
                          {m.auth_sign_in_link()}
                        </Link>
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
