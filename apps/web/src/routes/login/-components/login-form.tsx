import { zodResolver } from "@hookform/resolvers/zod";
import { Link, getRouteApi, useNavigate } from "@tanstack/react-router";
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

import { useLogin } from "@/hooks/use-auth";
import { useCapabilities } from "@/hooks/use-capabilities";
import { apiErrorMessage } from "@/lib/api-error";
import { loginSchema } from "@/lib/api/auth-schema";
import type { LoginFormValues } from "@/lib/api/auth-schema";
import { oidcErrorMessage } from "@/lib/api/identity-provider";
import { containerVariants, itemVariants } from "@/lib/motion";
import { safeRedirect } from "@/lib/safe-redirect";
import * as m from "@/paraglide/messages";

import { ProviderButtons } from "./provider-buttons";

/** 直接 import Route 會跟 index.tsx 形成循環依賴。 */
const routeApi = getRouteApi("/login/");

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

  const onSubmit = handleSubmit((values) => {
    login.mutate(values, {
      onSuccess: () => {
        // admin 是同網域的另一包 SPA，router 裡沒有它的路由，只能整頁跳轉。
        const target = safeRedirect(search.redirect, window.location.origin);
        if (target) {
          window.location.assign(target);
          return;
        }
        navigate({ to: "/" });
      },
    });
  });

  const formError = login.error ? apiErrorMessage(login.error) : null;

  /*
   * 第三方回來失敗時後端把人送回這頁並帶上代碼。認不得的代碼（後端加了新的、
   * 我們還沒跟上）退回一句通用的 —— 把生代碼放到畫面上對使用者沒有意義。
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
                  {m.auth_welcome_back()}
                </CardTitle>
              </motion.div>
              <motion.div variants={itemVariants}>
                <CardDescription>
                  {m.auth_sign_in_description()}
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
                        <Field data-invalid={fieldState.invalid}>
                          <div className="flex items-center">
                            <FieldLabel htmlFor="password">
                              {m.auth_password()}
                            </FieldLabel>
                            {/*
                              沒設寄信時這條路一定走不通（後端回 404），
                              跟隱藏註冊連結一樣的理由。讀不到 capabilities
                              時照常顯示 —— 少一個連結比多一個難發現。
                            */}
                            {capabilities?.password_reset === false ? null : (
                              <Link
                                className="ml-auto inline-block text-sm underline-offset-4 hover:underline"
                                to="/forgot-password"
                              >
                                {m.auth_forgot_password()}
                              </Link>
                            )}
                          </div>
                          <Input
                            {...field}
                            aria-invalid={fieldState.invalid}
                            id="password"
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
                      <ProviderButtons invite={search.invite} />
                      {/*
                        註冊關著就不要指一條走不通的路。讀不到 capabilities 時
                        照常顯示 —— 少一個連結比多一個難發現。
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
            </CardContent>
          </Card>
        </motion.div>
      </motion.div>
    </div>
  );
}
