import { IconShare } from "@tabler/icons-react";
import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";

import { PageHeader, PageShell } from "@/components/page-header";
import { requireAuth } from "@/lib/route-guard";
import * as m from "@/paraglide/messages";

import { MyShares } from "./-components/my-shares";

/**
 * The page number lives in search, not the path, so a reload or a saved link returns
 * to the same page. A 1-based `page` instead of `offset` keeps the URL readable.
 */
const searchSchema = z.object({
  // oxlint-disable-next-line promise/prefer-await-to-then -- zod's catch, not a Promise's
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
        <MyShares page={page} />
      </div>
    </PageShell>
  );
}
