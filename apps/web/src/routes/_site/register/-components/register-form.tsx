import { zodResolver } from "@hookform/resolvers/zod";
import { Link, getRouteApi, useNavigate } from "@tanstack/react-router";
import { useRegister } from "@tessera/auth/hooks";
import { Button, buttonVariants } from "@tessera/ui/components/button";
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
import { useMemo, useState } from "react";
import { Controller, useForm, useWatch } from "react-hook-form";

import { PasswordRequirements } from "@/components/password-requirements";
import { useCapabilities } from "@/hooks/use-capabilities";
import { usePasswordBreach } from "@/hooks/use-password-breach";
import { usePasswordPolicy } from "@/hooks/use-password-policy";
import { apiErrorMessage } from "@/lib/api-error";
import { registerSchemaFor } from "@/lib/api/auth-schema";
import type { RegisterFormValues } from "@/lib/api/auth-schema";
import { isBreachedPasswordError } from "@/lib/api/password-policy";
import { containerVariants, itemVariants } from "@/lib/motion";
import * as m from "@/paraglide/messages";

/** Importing Route directly would create a cycle with index.tsx (as in login-form). */
const routeApi = getRouteApi("/_site/register/");

export function RegisterForm({
  className,
  ...props
}: React.ComponentProps<"div">) {
  const navigate = useNavigate();
  const search = routeApi.useSearch();
  const registerAccount = useRegister();
  const policy = usePasswordPolicy();
  const { data: capabilities } = useCapabilities();
  // On a 202 (confirmation mail sent) this page shows a notice; null means not submitted
  // yet, or the account was created outright.
  const [pendingEmail, setPendingEmail] = useState<string | null>(null);

  /*
   * A code is required only for invite-only registration. With registration
   * open the server ignores the field, so requiring it here would invent a
   * rule this instance does not have. If capabilities cannot be read, assume
   * open: the most permissive guess, and the server still refuses if a code
   * is needed.
   */
  const requireInvite = capabilities?.registration === "invite";
  const schema = useMemo(
    () => registerSchemaFor(policy, requireInvite),
    [policy, requireInvite]
  );
  const { control, handleSubmit, setError } = useForm<RegisterFormValues>({
    defaultValues: {
      email: "",
      // Prefilled from the link, so it need not be copied by hand.
      inviteCode: search.invite ?? "",
      password: "",
      username: "",
    },
    resolver: zodResolver(schema),
  });
  const password = useWatch({ control, name: "password" });
  const breached = usePasswordBreach(password, policy);

  const onSubmit = handleSubmit(({ inviteCode, ...values }) => {
    // The password field already shows the breach; the server would only refuse it again.
    if (breached) {
      return;
    }
    registerAccount.mutate(
      {
        ...values,
        // An empty string is not sent. The server ignores it when registration
        // is open, but an empty value would make the request log suggest a
        // code was given.
        invite_code: inviteCode.trim() === "" ? undefined : inviteCode.trim(),
      },
      {
        onError: (error) => {
          if (isBreachedPasswordError(error)) {
            setError("password", {
              message: m.auth_password_breached(),
              type: "server",
            });
          }
        },
        onSuccess: (account) => {
          // `account` is undefined when the server sent a confirmation link
          // instead of creating one outright, so there is nothing to sign in
          // to yet. Going to the sign-in page would suggest otherwise.
          if (account) {
            navigate({ to: "/" });
            return;
          }
          setPendingEmail(values.email);
        },
      }
    );
  });

  // A breached password is already reported on the password field.
  const formError =
    registerAccount.error && !isBreachedPasswordError(registerAccount.error)
      ? apiErrorMessage(registerAccount.error)
      : null;

  /*
   * With registration closed the form should not appear at all: letting
   * someone fill it in only to get a 403 delays what the server already knew.
   * Provider sign-in is unaffected (existing accounts still get in), so this
   * points to the sign-in page instead of only saying it is closed.
   */
  if (capabilities?.registration === "closed") {
    return <RegistrationClosed className={className} {...props} />;
  }

  if (pendingEmail) {
    return (
      <RegistrationPending
        className={className}
        email={pendingEmail}
        {...props}
      />
    );
  }

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
                  {/*
                    At the top: it decides whether one can register at all,
                    and finding a missing code after filling in the rest is
                    the worst order. Not drawn when registration is open.
                  */}
                  {requireInvite && (
                    <motion.div variants={itemVariants}>
                      <Controller
                        control={control}
                        name="inviteCode"
                        render={({ field, fieldState }) => (
                          <Field data-invalid={fieldState.invalid}>
                            <FieldLabel htmlFor="invite-code">
                              {m.auth_invite_code()}
                            </FieldLabel>
                            <Input
                              {...field}
                              aria-invalid={fieldState.invalid}
                              autoComplete="off"
                              id="invite-code"
                            />
                            <FieldDescription>
                              {m.auth_invite_description()}
                            </FieldDescription>
                            {fieldState.invalid && (
                              <FieldError errors={[fieldState.error]} />
                            )}
                          </Field>
                        )}
                      />
                    </motion.div>
                  )}
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
                    <Field>
                      {formError && <FieldError>{formError}</FieldError>}
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

/** After a 202, waiting for the user to click the confirmation link in their mail. */
function RegistrationPending({
  className,
  email,
  ...props
}: React.ComponentProps<"div"> & { email: string }) {
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
              <CardTitle className="font-medium text-xl tracking-tight">
                {m.auth_register_pending_title()}
              </CardTitle>
              <CardDescription>
                {m.auth_register_pending_description({ email })}
              </CardDescription>
            </CardHeader>
            <CardContent>
              <Link
                className={cn(buttonVariants({ variant: "outline" }), "w-full")}
                to="/login"
              >
                {m.auth_back_to_login()}
              </Link>
            </CardContent>
          </Card>
        </motion.div>
      </motion.div>
    </div>
  );
}

/** Replaces the form while registration is closed. */
function RegistrationClosed({
  className,
  ...props
}: React.ComponentProps<"div">) {
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
              <CardTitle className="font-medium text-xl tracking-tight">
                {m.auth_registration_closed_title()}
              </CardTitle>
              <CardDescription>
                {m.auth_registration_closed_description()}
              </CardDescription>
            </CardHeader>
            <CardContent>
              <Link
                className={cn(buttonVariants({ variant: "outline" }), "w-full")}
                to="/login"
              >
                {m.auth_back_to_login()}
              </Link>
            </CardContent>
          </Card>
        </motion.div>
      </motion.div>
    </div>
  );
}
