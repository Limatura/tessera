import { zodResolver } from "@hookform/resolvers/zod";
import { Link, getRouteApi, useNavigate } from "@tanstack/react-router";
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
import { useRegister } from "@/hooks/use-auth";
import { useCapabilities } from "@/hooks/use-capabilities";
import { usePasswordBreach } from "@/hooks/use-password-breach";
import { usePasswordPolicy } from "@/hooks/use-password-policy";
import { apiErrorMessage } from "@/lib/api-error";
import { registerSchemaFor } from "@/lib/api/auth-schema";
import type { RegisterFormValues } from "@/lib/api/auth-schema";
import { isBreachedPasswordError } from "@/lib/api/password-policy";
import { containerVariants, itemVariants } from "@/lib/motion";
import * as m from "@/paraglide/messages";

/** 直接 import Route 會跟 index.tsx 形成循環依賴(同 login-form)。 */
const routeApi = getRouteApi("/register/");

export function RegisterForm({
  className,
  ...props
}: React.ComponentProps<"div">) {
  const navigate = useNavigate();
  const search = routeApi.useSearch();
  const registerAccount = useRegister();
  const policy = usePasswordPolicy();
  const { data: capabilities } = useCapabilities();
  // 202（寄確認信）時留在這頁顯示提示；undefined 代表還沒送出或已直接建立帳號。
  const [pendingEmail, setPendingEmail] = useState<string | null>(null);

  /*
   * 邀請制才要求填碼。開放註冊時 server 根本不看這個欄位,在這裡擋人等於發明一條
   * 這台 instance 沒有的規則。讀不到 capabilities 就當開放 —— 那是最寬鬆的假設,
   * 真的需要碼的話 server 會退回來,不會讓人矇混過去。
   */
  const requireInvite = capabilities?.registration === "invite";
  const schema = useMemo(
    () => registerSchemaFor(policy, requireInvite),
    [policy, requireInvite]
  );
  const { control, handleSubmit, setError } = useForm<RegisterFormValues>({
    defaultValues: {
      email: "",
      // 連結裡帶了就填好,不必再手抄一次。
      inviteCode: search.invite ?? "",
      password: "",
      username: "",
    },
    resolver: zodResolver(schema),
  });
  const password = useWatch({ control, name: "password" });
  const breached = usePasswordBreach(password, policy);

  const onSubmit = handleSubmit(({ inviteCode, ...values }) => {
    // 密碼欄下面已經標出外洩了,送出去 server 也只會再退回一次。
    if (breached) {
      return;
    }
    registerAccount.mutate(
      {
        ...values,
        // 空字串不送 —— server 在開放註冊時會忽略它,但送一個空值進去只會讓
        // 之後讀請求紀錄的人以為這裡真的有碼。
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
          // instead of creating one outright — nothing to sign in to yet.
          // 直接跳登入頁會讓人以為已經能登入，實際上帳號根本不存在。
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
   * 註冊關掉時整張表都不該出現 —— 讓人填完八個欄位再吃一個 403,是把 server 早就
   * 知道的事拖到最後一刻才講。第三方登入不受這個影響(既有帳號照樣進得來),
   * 所以送去登入頁而不是一句「不開放」了事。
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
                    擺在最上面:它是進得來進不來的前提,填完底下一整張表才發現
                    少一個碼是最差的順序。開放註冊時整格不畫。
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

/** 已經送出 202，等使用者去信箱點確認連結。 */
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

/** 註冊關著的時候頂替整張表。 */
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
