import { IconPlus, IconTrash } from "@tabler/icons-react";
import { createFileRoute } from "@tanstack/react-router";
import type { components } from "@tessera/api/schema";
import { useSession } from "@tessera/auth/hooks";
import { Badge } from "@tessera/ui/components/badge";
import { Button } from "@tessera/ui/components/button";
import {
  Card,
  CardAction,
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
import { useState } from "react";

import { useInvites } from "@/hooks/admin/use-admin";
import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import { hasPermission, PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

import { CreateInviteDialog } from "./-components/create-invite-dialog";
import { RevokeInviteDialog } from "./-components/revoke-invite-dialog";

type Invite = components["schemas"]["InviteResponse"];

export const Route = createFileRoute("/admin/invites/")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.invitesRead
    ),
  component: InvitesComponent,
  staticData: { title: m.invites_page_title },
});

/** 邀請碼本身讀不回來，表格跟撤銷對話框只能靠備註認出是哪一組。 */
function inviteLabel(invite: Invite): string {
  return invite.note ?? m.invites_unnamed();
}

function InvitesComponent() {
  const invites = useInvites();
  const formatDate = useDateTimeFormatter();
  const { data: account } = useSession();
  const [creating, setCreating] = useState(false);
  const [revoking, setRevoking] = useState<Invite | null>(null);

  const canCreate = hasPermission(account, PERMISSIONS.invitesCreate);
  const canRevoke = hasPermission(account, PERMISSIONS.invitesDelete);

  return (
    <>
      <Card className="ring-foreground/10">
        <CardHeader>
          <CardTitle>{m.invites_card_title()}</CardTitle>
          <CardDescription>{m.invites_card_description()}</CardDescription>
          {canCreate && (
            <CardAction>
              <Button onClick={() => setCreating(true)} type="button">
                <IconPlus />
                {m.invites_create_button()}
              </Button>
            </CardAction>
          )}
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
          {invites.data?.length === 0 && (
            <p className="text-sm text-muted-foreground">{m.invites_empty()}</p>
          )}
          {invites.data && invites.data.length > 0 && (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{m.invites_col_note()}</TableHead>
                  <TableHead>{m.invites_col_status()}</TableHead>
                  <TableHead className="text-right">
                    {m.invites_col_usage()}
                  </TableHead>
                  <TableHead>{m.invites_col_expires()}</TableHead>
                  {canRevoke && <TableHead className="w-12" />}
                </TableRow>
              </TableHeader>
              <TableBody>
                {invites.data.map((invite) => (
                  <TableRow key={invite.id}>
                    <TableCell className="font-medium">
                      {inviteLabel(invite)}
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
                      {invite.expires_at
                        ? formatDate(invite.expires_at)
                        : m.common_never_expires()}
                    </TableCell>
                    {canRevoke && (
                      <TableCell>
                        {/* 已失效的也給：撤銷是刪掉這一列，用完次數的不會被自動清掉。 */}
                        <Button
                          aria-label={m.invites_revoke_aria({
                            label: inviteLabel(invite),
                          })}
                          onClick={() => setRevoking(invite)}
                          size="icon"
                          type="button"
                          variant="ghost"
                        >
                          <IconTrash />
                        </Button>
                      </TableCell>
                    )}
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      {canCreate && (
        <CreateInviteDialog onOpenChange={setCreating} open={creating} />
      )}
      {canRevoke && (
        <RevokeInviteDialog
          invite={revoking}
          label={revoking ? inviteLabel(revoking) : ""}
          onOpenChange={(open) => {
            if (!open) {
              setRevoking(null);
            }
          }}
        />
      )}
    </>
  );
}
