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
import { useInvites } from "@/hooks/admin/use-admin";
import { PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

export const Route = createFileRoute("/admin/invites")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.invitesRead
    ),
  component: InvitesComponent,
});

const dateFormatter = new Intl.DateTimeFormat("zh-TW", {
  dateStyle: "medium",
  timeStyle: "short",
});

function formatDate(value: string | null | undefined): string {
  return value
    ? dateFormatter.format(new Date(value))
    : m.common_never_expires();
}

function InvitesComponent() {
  const invites = useInvites();

  return (
    <AdminShell title={m.invites_page_title()}>
      <Card className="ring-foreground/10">
        <CardHeader>
          <CardTitle>{m.invites_card_title()}</CardTitle>
          <CardDescription>{m.invites_card_description()}</CardDescription>
        </CardHeader>
        <CardContent>
          {invites.isPending && (
            <p className="text-sm text-muted-foreground">
              {m.common_loading()}
            </p>
          )}
          {invites.isError && (
            <p className="text-sm text-destructive">{m.invites_load_error()}</p>
          )}
          {invites.data && (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{m.invites_col_note()}</TableHead>
                  <TableHead>{m.invites_col_status()}</TableHead>
                  <TableHead className="text-right">
                    {m.invites_col_usage()}
                  </TableHead>
                  <TableHead>{m.invites_col_expires()}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {invites.data.map((invite) => (
                  <TableRow key={invite.id}>
                    <TableCell className="font-medium">
                      {invite.note ?? m.common_dash_placeholder()}
                    </TableCell>
                    <TableCell>
                      <Badge variant={invite.active ? "secondary" : "outline"}>
                        {invite.active
                          ? m.invites_status_active()
                          : m.invites_status_inactive()}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {invite.used} / {invite.max_uses}
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {formatDate(invite.expires_at)}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </AdminShell>
  );
}
