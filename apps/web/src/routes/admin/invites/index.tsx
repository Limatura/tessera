import { IconPlus, IconTrash } from "@tabler/icons-react";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
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
import { cn } from "@tessera/ui/lib/utils";
import { useState } from "react";
import { z } from "zod";

import {
  ListEmpty,
  ListError,
  ListSkeleton,
} from "@/components/admin/list-state";
import { useInvites } from "@/hooks/admin/use-invites";
import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import { hasPermission, PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

import { CreateInviteDialog } from "./-components/create-invite-dialog";
import { RevokeInviteDialog } from "./-components/revoke-invite-dialog";

type Invite = components["schemas"]["InviteResponse"];

/** `?create` opens the create dialog on arrival, for the command menu's "Create invite". */
const invitesSearchSchema = z.object({
  // oxlint-disable-next-line promise/prefer-await-to-then -- zod's catch, not a Promise's
  create: z.boolean().optional().catch(false),
});

export const Route = createFileRoute("/admin/invites/")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.invitesRead
    ),
  component: InvitesComponent,
  staticData: { title: m.invites_page_title },
  validateSearch: invitesSearchSchema,
});

/** The code cannot be read back, so the table and revoke dialog recognise an invite by its note. */
function inviteLabel(invite: Invite): string {
  return invite.note ?? m.invites_unnamed();
}

function InvitesComponent() {
  const invites = useInvites();
  const { data: account } = useSession();
  const { create } = Route.useSearch();
  const navigate = useNavigate({ from: Route.fullPath });
  const [creatingHere, setCreatingHere] = useState(false);
  const [revoking, setRevoking] = useState<Invite | null>(null);

  const canCreate = hasPermission(account, PERMISSIONS.invitesCreate);
  const canRevoke = hasPermission(account, PERMISSIONS.invitesDelete);
  const creating = creatingHere || (create === true && canCreate);

  // Closing drops `?create` too, so going back does not reopen the dialog.
  const handleCreatingChange = (open: boolean) => {
    setCreatingHere(open);
    if (!open && create) {
      navigate({ replace: true, search: {} });
    }
  };

  return (
    <>
      <Card className="ring-foreground/10">
        <CardHeader>
          <CardTitle>{m.invites_card_title()}</CardTitle>
          <CardDescription>{m.invites_card_description()}</CardDescription>
          {canCreate && (
            <CardAction>
              <Button onClick={() => setCreatingHere(true)} type="button">
                <IconPlus />
                {m.invites_create_button()}
              </Button>
            </CardAction>
          )}
        </CardHeader>
        <CardContent>
          {invites.isPending && <ListSkeleton rows={3} />}
          {invites.isError && (
            <ListError
              isRetrying={invites.isFetching}
              message={m.invites_load_error()}
              onRetry={() => invites.refetch()}
            />
          )}
          {invites.data?.length === 0 && (
            <ListEmpty>{m.invites_empty()}</ListEmpty>
          )}
          {invites.data && invites.data.length > 0 && (
            <>
              <ul className="flex flex-col gap-3 md:hidden">
                {invites.data.map((invite) => (
                  <InviteCard
                    invite={invite}
                    key={invite.id}
                    onRevoke={canRevoke ? () => setRevoking(invite) : null}
                  />
                ))}
              </ul>
              <div className="hidden md:block">
                <InvitesTable
                  invites={invites.data}
                  onRevoke={canRevoke ? setRevoking : null}
                />
              </div>
            </>
          )}
        </CardContent>
      </Card>

      {canCreate && (
        <CreateInviteDialog
          onOpenChange={handleCreatingChange}
          open={creating}
        />
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

/** The `md` and up layout. */
function InvitesTable({
  invites,
  onRevoke,
}: {
  invites: Invite[];
  /** `null`: no `invites:delete`, so no revoke column. */
  onRevoke: ((invite: Invite) => void) | null;
}) {
  const formatDate = useDateTimeFormatter();

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>{m.invites_col_note()}</TableHead>
          <TableHead>{m.invites_col_status()}</TableHead>
          <TableHead className="text-right">{m.invites_col_usage()}</TableHead>
          <TableHead>{m.invites_col_expires()}</TableHead>
          {onRevoke && <TableHead className="w-12" />}
        </TableRow>
      </TableHeader>
      <TableBody>
        {invites.map((invite) => (
          <TableRow key={invite.id}>
            <TableCell className="font-medium">{inviteLabel(invite)}</TableCell>
            <TableCell>
              <InviteStatus invite={invite} />
            </TableCell>
            <TableCell className="text-right tabular-nums">
              {invite.used} / {invite.max_uses}
            </TableCell>
            <TableCell className="text-muted-foreground">
              {invite.expires_at
                ? formatDate(invite.expires_at)
                : m.common_never_expires()}
            </TableCell>
            {onRevoke && (
              <TableCell>
                <RevokeButton
                  invite={invite}
                  onClick={() => onRevoke(invite)}
                />
              </TableCell>
            )}
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

/** The narrow-screen layout: note and status on top, usage and expiry below. */
function InviteCard({
  invite,
  onRevoke,
}: {
  invite: Invite;
  onRevoke: (() => void) | null;
}) {
  const formatDate = useDateTimeFormatter();

  return (
    <li
      className={cn(
        "rounded-xl border bg-card",
        !invite.active && "border-dashed bg-muted/30"
      )}
    >
      <div className="flex items-start gap-3 p-4 pb-3">
        <div className="flex min-w-0 flex-1 flex-col gap-2">
          <p
            className={cn(
              "truncate font-medium text-sm",
              !invite.active && "text-muted-foreground"
            )}
          >
            {inviteLabel(invite)}
          </p>
          <span>
            <InviteStatus invite={invite} />
          </span>
        </div>
        {onRevoke && (
          <RevokeButton
            className="-mt-2 -mr-2 size-10 shrink-0"
            invite={invite}
            onClick={onRevoke}
          />
        )}
      </div>
      <dl className="grid grid-cols-2 gap-x-4 gap-y-1 border-t px-4 py-3 text-xs tabular-nums">
        <div className="flex flex-col gap-0.5">
          <dt className="text-muted-foreground">{m.invites_col_usage()}</dt>
          <dd>
            {invite.used} / {invite.max_uses}
          </dd>
        </div>
        <div className="flex flex-col gap-0.5">
          <dt className="text-muted-foreground">{m.invites_col_expires()}</dt>
          <dd>
            {invite.expires_at
              ? formatDate(invite.expires_at)
              : m.common_never_expires()}
          </dd>
        </div>
      </dl>
    </li>
  );
}

function InviteStatus({ invite }: { invite: Invite }) {
  return (
    <Badge variant={invite.active ? "secondary" : "outline"}>
      {invite.active ? m.invites_status_active() : m.invites_status_inactive()}
    </Badge>
  );
}

/** Dead invites too: revoking deletes the row, and used-up ones are not cleared automatically. */
function RevokeButton({
  invite,
  onClick,
  className,
}: {
  invite: Invite;
  onClick: () => void;
  className?: string;
}) {
  return (
    <Button
      aria-label={m.invites_revoke_aria({ label: inviteLabel(invite) })}
      className={className}
      onClick={onClick}
      size="icon"
      type="button"
      variant="ghost"
    >
      <IconTrash />
    </Button>
  );
}
