import { IconSettings, IconUserCircle } from "@tabler/icons-react";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { Tabs, TabsList, TabsTrigger } from "@tessera/ui/components/tabs";
import { z } from "zod";

import { PageHeader, PageShell } from "@/components/page-header";
import { requireAuth } from "@/lib/route-guard";
import * as m from "@/paraglide/messages";

import { AccountTab } from "./-components/account-tab";
import { PreferencesTab } from "./-components/preferences-tab";

const TABS = ["account", "preferences"] as const;

/** 分頁放在 search 而不是路徑,重新整理跟分享連結都會回到同一頁。 */
const searchSchema = z.object({
  /** 綁定第三方失敗時後端帶回來的代碼；參數名跟登入頁共用。 */
  oidc_error: z.string().optional(),
  // oxlint-disable-next-line promise/prefer-await-to-then -- zod 的 catch,不是 Promise 的
  tab: z.enum(TABS).catch("account"),
});

export const Route = createFileRoute("/_site/user/")({
  beforeLoad: ({ context, location }) =>
    requireAuth(context.queryClient, location.href),
  component: UserPage,
  validateSearch: searchSchema,
});

function UserPage() {
  const { tab } = Route.useSearch();
  const navigate = useNavigate({ from: Route.fullPath });

  const handleTabChange = (value: unknown) => {
    // 換分頁時把 oidc_error 丟掉 —— 它講的是剛才那次綁定，不該跟著人到處走。
    navigate({ search: { tab: value as (typeof TABS)[number] } });
  };

  return (
    <PageShell>
      <PageHeader
        description={m.user_page_description()}
        icon={<IconUserCircle size={20} />}
        title={m.user_page_title()}
      />

      <Tabs className="mt-6" onValueChange={handleTabChange} value={tab}>
        <TabsList>
          <TabsTrigger value="account">
            <IconUserCircle size={16} />
            {m.user_tab_account()}
          </TabsTrigger>
          <TabsTrigger value="preferences">
            <IconSettings size={16} />
            {m.user_tab_preferences()}
          </TabsTrigger>
        </TabsList>
      </Tabs>

      <div className="mt-6">
        {tab === "account" ? <AccountTab /> : <PreferencesTab />}
      </div>
    </PageShell>
  );
}
