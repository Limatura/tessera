import { IconShare } from "@tabler/icons-react";
import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";

import { PageHeader, PageShell } from "@/components/page-header";
import { requireAuth } from "@/lib/route-guard";
import * as m from "@/paraglide/messages";

import { SharesTable } from "./-components/shares-table";

/**
 * 頁碼放 search 而不是路徑,重新整理跟貼給自己的連結都會回到同一頁。
 * 用 1-based 的 `page` 而不是 `offset`,網址對人來說才讀得懂。
 */
const searchSchema = z.object({
  // oxlint-disable-next-line promise/prefer-await-to-then -- zod 的 catch,不是 Promise 的
  page: z.coerce.number().int().min(1).catch(1),
});

export const Route = createFileRoute("/_site/shares/")({
  beforeLoad: ({ context, location }) =>
    requireAuth(context.queryClient, location.href),
  component: SharesPage,
  validateSearch: searchSchema,
});

function SharesPage() {
  const { page } = Route.useSearch();

  return (
    <PageShell>
      <PageHeader
        description={m.shares_page_description()}
        icon={<IconShare size={20} />}
        title={m.shares_page_title()}
      />
      <div className="mt-6">
        <SharesTable page={page} />
      </div>
    </PageShell>
  );
}
