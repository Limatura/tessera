import { createRoute, redirect } from "@tanstack/react-router";

import { landingRoute } from "@/lib/permissions";
import { requireAuth } from "@/lib/route-guard";

import { Route as rootRoute } from "./__root";

/**
 * 後台首頁本身不顯示任何東西，只負責把人送到他權限內的第一個頁面。
 *
 * 權限少的人進儀表板會是一片空白，所以登入後要往下找第一個進得去的；都沒有就落到
 * 「我的分享」（見 lib/permissions.ts 的 LANDING_ORDER）。
 */
export const Route = createRoute({
  beforeLoad: async ({ context, location }) => {
    const { account } = await requireAuth(context.queryClient, location.href);
    throw redirect({ to: landingRoute(account) });
  },
  getParentRoute: () => rootRoute,
  path: "/",
});
