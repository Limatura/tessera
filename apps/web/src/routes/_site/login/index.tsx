import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";

import { GlassBackground } from "@/components/glass-background";

import { LoginForm } from "./-components/login-form";

/** 要登入的頁面（包括後台）會把人送來這裡，並帶上原本要去的位址。 */
const searchSchema = z.object({
  /** 有邀請碼的話要一路帶到 `start`，後端在成功建立帳號時才花掉它。 */
  invite: z.string().optional(),
  /** 第三方登入失敗時後端帶回來的代碼；參數名三家共用，沒跟著 provider 改。 */
  oidc_error: z.string().optional(),
  redirect: z.string().optional(),
});

export const Route = createFileRoute("/_site/login/")({
  component: RouteComponent,
  validateSearch: searchSchema,
});

function RouteComponent() {
  return (
    <div className="flex min-h-svh w-full items-center justify-center p-6 md:p-10">
      <GlassBackground />
      <div className="w-full max-w-sm">
        <LoginForm />
      </div>
    </div>
  );
}
