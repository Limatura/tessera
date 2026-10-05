import { createFileRoute, redirect } from "@tanstack/react-router";
import { z } from "zod";

/**
 * 後端在第三方綁定回來時把人送到 `/account`（見 `apps/server/src/api/oidc.rs`
 * 的 `ACCOUNT_PAGE`），但這包 SPA 的帳號頁在 `/user`。這裡只做轉址，把後端帶回
 * 來的 `oidc_error` 一起帶過去 —— 不然那次綁定失敗的原因會在轉址時掉光。
 *
 * 後端那個常數改成 `/user` 之後這支就可以刪掉。留著也不會壞，只是多一個入口。
 */
const searchSchema = z.object({
  oidc_error: z.string().optional(),
});

export const Route = createFileRoute("/_site/account")({
  beforeLoad: ({ search }) => {
    throw redirect({
      search: { oidc_error: search.oidc_error, tab: "account" as const },
      to: "/user",
    });
  },
  validateSearch: searchSchema,
});
