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
  /** Where to go afterwards; may be a full URL sent from the admin panel. */
  redirect: z.string().optional(),
});

/**
 * The only page reachable when a password change is required
 * (`must_change_password`); the root guard and the global error handler both
 * send users here. Anyone else may use it too, though they normally use the
 * section on the account page.
 */
export const Route = createFileRoute("/_site/change-password/")({
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
    const target = safeRedirect(search.redirect, window.location.origin);
    if (target) {
      navigate({ href: target });
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
