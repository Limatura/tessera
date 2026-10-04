import { IconLock } from "@tabler/icons-react";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import { z } from "zod";

import { ChangePasswordForm } from "@/components/change-password-form";
import { GlassBackground } from "@/components/glass-background";
import { requireAuth } from "@/lib/route-guard";
import { safeRedirect } from "@/lib/safe-redirect";
import * as m from "@/paraglide/messages";

const searchSchema = z.object({
  /** 改完要回去的地方；可能是 admin 送過來的完整網址。 */
  redirect: z.string().optional(),
});

/**
 * 強制改密碼（`must_change_password`）時唯一能進的頁面，root 的 guard 和全域錯誤
 * 處理都把人送到這裡。沒被強制的人也能用，只是平常會走個人設定頁的那一塊。
 */
export const Route = createFileRoute("/change-password/")({
  beforeLoad: ({ context, location }) =>
    requireAuth(context.queryClient, location.href),
  component: ChangePasswordPage,
  validateSearch: searchSchema,
});

function ChangePasswordPage() {
  const { account } = Route.useRouteContext();
  const search = Route.useSearch();
  const navigate = useNavigate();

  const handleSuccess = () => {
    // admin 是同網域的另一包 SPA，router 裡沒有它的路由，只能整頁跳轉。
    const target = safeRedirect(search.redirect, window.location.origin);
    if (target) {
      window.location.assign(target);
      return;
    }
    navigate({ to: "/" });
  };

  return (
    <div className="flex min-h-svh w-full items-center justify-center p-6 md:p-10">
      <GlassBackground />
      <div className="w-full max-w-sm">
        <Card className="border-border/60 bg-card/60 shadow-2xl shadow-black/5 backdrop-blur-2xl">
          <CardHeader>
            <CardTitle className="flex items-center gap-2 font-medium text-xl tracking-tight">
              <IconLock size={20} />
              {m.change_password_title()}
            </CardTitle>
            <CardDescription>
              {account.must_change_password
                ? m.change_password_required_description()
                : m.user_password_description()}
            </CardDescription>
          </CardHeader>
          <CardContent>
            <ChangePasswordForm onSuccess={handleSuccess} variant="page" />
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
