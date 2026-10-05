import { createFileRoute, redirect } from "@tanstack/react-router";

import { landingRoute } from "@/lib/admin/permissions";
import { requireAuth } from "@/lib/route-guard";

/**
 * 後台首頁本身不顯示任何東西，只負責把人送到他權限內的第一個頁面。
 *
 * 權限少的人進儀表板會是一片空白，所以登入後要往下找第一個進得去的（見
 * lib/admin/permissions.ts 的 LANDING_ORDER）；一頁都進不去的就送回前台。
 */
export const Route = createFileRoute("/admin/")({
  beforeLoad: async ({ context, location }) => {
    const { account } = await requireAuth(context.queryClient, location.href);
    const to = landingRoute(account);
    if (to === null) {
      throw redirect({ to: "/" });
    }
    throw redirect({ to });
  },
});
