import { zodResolver } from "@hookform/resolvers/zod";
import { Link, getRouteApi, useNavigate } from "@tanstack/react-router";
import { useLogin } from "@tessera/auth/hooks";
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
import { useState } from "react";
import { Controller, useForm } from "react-hook-form";

import { useCapabilities } from "@/hooks/use-capabilities";
import { apiErrorMessage } from "@/lib/api-error";
import { loginSchema } from "@/lib/api/auth-schema";
import type { LoginFormValues } from "@/lib/api/auth-schema";
import { oidcErrorMessage } from "@/lib/api/identity-provider";
import { containerVariants, itemVariants } from "@/lib/motion";
import { safeRedirect } from "@/lib/safe-redirect";
import * as m from "@/paraglide/messages";

import { MfaCodeStep } from "./mfa-code-step";
import { PasskeySignInButton } from "./passkey-sign-in-button";
import { ProviderButtons } from "./provider-buttons";

/** Importing Route directly would create a cycle with index.tsx. */
const routeApi = getRouteApi("/_site/login/");

export function LoginForm({
  className,
  ...props
}: React.ComponentProps<"div">) {
  const navigate = useNavigate();
  const search = routeApi.useSearch();
  const login = useLogin();
  const { data: capabilities } = useCapabilities();
  const { control, handleSubmit } = useForm<LoginFormValues>({
    defaultValues: { identifier: "", password: "" },
    resolver: zodResolver(loginSchema),
  });

  // Set when the password was right and the account wants a code as well.
  const [ticket, setTicket] = useState<string | null>(null);

  const handleSignedIn = () => {
    const target = safeRedirect(search.redirect, window.location.origin);
    if (target) {
      navigate({ href: target });
      return;
    }
    navigate({ to: "/" });
  };

  const handleRestart = () => {
    setTicket(null);
    login.reset();
  };

  const onSubmit = handleSubmit((values) => {
    login.mutate(values, {
      onSuccess: (result) => {
        if (result.status === "mfa_required") {
          setTicket(result.challenge.ticket);
          return;
        }
        handleSignedIn();
      },
    });
  });

  const formError = login.error ? apiErrorMessage(login.error) : null;

  /*
   * When a provider sign-in fails the server sends the user back here with a
   * code. An unknown code (one the server added that we have not caught up
   * with) falls back to a generic sentence; a raw code means nothing to users.
   */
  const providerError = search.oidc_error
    ? (oidcErrorMessage(search.oidc_error) ?? m.oidc_error_generic())
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
                  {ticket ? m.auth_mfa_title() : m.auth_welcome_back()}
                </CardTitle>
              </motion.div>
              <motion.div variants={itemVariants}>
                <CardDescription>
                  {ticket
                    ? m.auth_mfa_description()
                    : m.auth_sign_in_description()}
                </CardDescription>
              </motion.div>
            </CardHeader>
            <CardContent>
              {providerError ? (
                <motion.p
                  className="mb-4 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-destructive text-sm"
                  role="alert"
                  variants={itemVariants}
                >
                  {providerError}
                </motion.p>
              ) : null}
              {ticket ? (
                <MfaCodeStep
                  onRestart={handleRestart}
                  onSignedIn={handleSignedIn}
                  ticket={ticket}
                />
              ) : (
                <form onSubmit={onSubmit}>
                  <FieldGroup>
                    <motion.div variants={itemVariants}>
                      <Controller
                        control={control}
                        name="identifier"
                        render={({ field, fieldState }) => (
                          <Field data-invalid={fieldState.invalid}>
                            <FieldLabel htmlFor="identifier">
                              {m.auth_username_or_email()}
                            </FieldLabel>
                            <Input
                              {...field}
                              aria-invalid={fieldState.invalid}
                              id="identifier"
                              placeholder="m@example.com"
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
                          <Field
                            className="relative"
                            data-invalid={fieldState.invalid}
                          >
                            <FieldLabel htmlFor="password">
                              {m.auth_password()}
                            </FieldLabel>
                            <Input
                              {...field}
                              aria-invalid={fieldState.invalid}
                              id="password"
                              type="password"
                            />
                            {fieldState.invalid && (
                              <FieldError errors={[fieldState.error]} />
                            )}
                            {/*
                              After the input in the DOM so Tab goes
                              identifier -> password -> this link, but drawn
                              on the label row. Without mail this path cannot
                              work (the server answers 404), the same reason
                              the registration link is hidden. If capabilities
                              cannot be read it shows anyway: a missing link
                              is harder to notice than an extra one.
                            */}
                            {capabilities?.password_reset === false ? null : (
                              <Link
                                className="absolute top-0 right-0 w-auto! text-sm underline-offset-4 hover:underline"
                                to="/forgot-password"
                              >
                                {m.auth_forgot_password()}
                              </Link>
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
                            disabled={login.isPending}
                            type="submit"
                          >
                            {login.isPending
                              ? m.auth_signing_in()
                              : m.auth_sign_in()}
                          </Button>
                        </motion.div>
                        <PasskeySignInButton onSignedIn={handleSignedIn} />
                        <ProviderButtons
                          invite={search.invite}
                          returnTo={
                            safeRedirect(
                              search.redirect,
                              window.location.origin
                            ) ?? undefined
                          }
                        />
                        {/*
                        With registration closed, do not point at a dead end.
                        If capabilities cannot be read it shows anyway: a
                        missing link is harder to notice than an extra one.
                      */}
                        {capabilities?.registration === "closed" ? null : (
                          <FieldDescription className="text-center">
                            {m.auth_no_account()}{" "}
                            <Link
                              className="underline-offset-4 hover:underline"
                              to="/register"
                            >
                              {m.auth_sign_up_link()}
                            </Link>
                          </FieldDescription>
                        )}
                      </Field>
                    </motion.div>
                  </FieldGroup>
                </form>
              )}
            </CardContent>
          </Card>
        </motion.div>
      </motion.div>
    </div>
  );
}
