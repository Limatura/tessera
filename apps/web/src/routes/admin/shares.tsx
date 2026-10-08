import { createFileRoute } from "@tanstack/react-router";
import { Badge } from "@tessera/ui/components/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@tessera/ui/components/table";

import { AdminShell } from "@/components/admin/admin-shell";
import { Pager } from "@/components/admin/pager";
import { SHARES_PAGE_SIZE, useMyShares } from "@/hooks/use-shares";
import { pageSearchSchema } from "@/lib/admin/page-search";
import { requireAuth } from "@/lib/route-guard";
import * as m from "@/paraglide/messages";

/**
 * 權限最少的人的落腳頁 —— 只要登入就進得來，不需要任何權限。
 * 顯示的是自己上傳的東西，不是全站的。
 */
export const Route = createFileRoute("/admin/shares")({
  beforeLoad: ({ context, location }) =>
    requireAuth(context.queryClient, location.href),
  component: SharesComponent,
  validateSearch: pageSearchSchema,
});

const dateFormatter = new Intl.DateTimeFormat("zh-TW", {
  dateStyle: "medium",
  timeStyle: "short",
});

function formatDate(value: string | null | undefined): string {
  return value
    ? dateFormatter.format(new Date(value))
    : m.common_dash_placeholder();
}

function SharesComponent() {
  const { page } = Route.useSearch();
  const shares = useMyShares((page - 1) * SHARES_PAGE_SIZE);
  const rows = shares.data?.data;

  return (
    <AdminShell title={m.admin_shares_page_title()}>
      <Card className="ring-foreground/10">
        <CardHeader>
          <CardTitle>{m.shares_card_title()}</CardTitle>
          <CardDescription>
            {shares.data
              ? m.shares_card_description_total({
                  total: shares.data.meta.total,
                })
              : m.shares_card_description_empty()}
          </CardDescription>
        </CardHeader>
        <CardContent>
          {shares.isPending && (
            <p className="text-sm text-muted-foreground">
              {m.common_loading()}
            </p>
          )}
          {shares.isError && (
            <p className="text-sm text-destructive">{m.shares_load_error()}</p>
          )}
          {rows?.length === 0 && (
            <p className="text-sm text-muted-foreground">
              {m.admin_shares_empty()}
            </p>
          )}
          {rows && rows.length > 0 && (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{m.admin_shares_col_code()}</TableHead>
                  <TableHead>{m.admin_shares_col_kind()}</TableHead>
                  <TableHead>{m.shares_col_created()}</TableHead>
                  <TableHead>{m.shares_col_last_accessed()}</TableHead>
                  <TableHead>{m.shares_col_expires()}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {rows.map((share) => (
                  <TableRow key={share.id}>
                    <TableCell className="font-mono font-medium">
                      {share.code}
                    </TableCell>
                    <TableCell>
                      <Badge variant="secondary">
                        {shareKindLabel(share.kind)}
                      </Badge>
                      {share.has_password && (
                        <Badge className="ml-1" variant="outline">
                          {m.admin_shares_needs_password()}
                        </Badge>
                      )}
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {formatDate(share.created_at)}
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {formatDate(share.last_accessed_at)}
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {share.expires_at
                        ? formatDate(share.expires_at)
                        : m.common_never_expires()}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
          {shares.data && (
            <Pager
              page={page}
              pageSize={SHARES_PAGE_SIZE}
              to="/admin/shares"
              total={shares.data.meta.total}
            />
          )}
        </CardContent>
      </Card>
    </AdminShell>
  );
}

/** API 的 `kind` 是英文代碼,顯示前翻成介面語言;未知值原樣顯示,後端加新類型時不會空白。 */
function shareKindLabel(kind: string): string {
  switch (kind) {
    case "text": {
      return m.shares_kind_text();
    }
    case "file": {
      return m.shares_kind_file();
    }
    case "link": {
      return m.shares_kind_link();
    }
    default: {
      return kind;
    }
  }
}
