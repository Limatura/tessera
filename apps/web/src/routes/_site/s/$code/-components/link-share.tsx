import type { components } from "@tessera/api/schema";
import { Button } from "@tessera/ui/components/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import { ExternalLink, Link2 } from "lucide-react";
import { useEffect, useState } from "react";

import { useShareTicket } from "@/hooks/use-share";
import { buildTransferUrl } from "@/lib/api/share";
import * as m from "@/paraglide/messages";

import { ShareMeta } from "./share-meta";

type PublicShare = components["schemas"]["PublicShareResponse"];

/**
 * A link share only lands here with a password or a retrieval limit; otherwise the
 * server answers with a 302.
 *
 * No ticket is fetched and `/raw` is not read to show the destination: both spend a
 * retrieval, and a crawler such as Discord's unfurling the URL would use up a one-time
 * link. So a limited link is spent only when someone presses "Go". With a password,
 * typing it was the intent, so unlocking redirects straight away.
 */
export function LinkShare({
  code,
  data,
  password,
}: {
  code: string;
  data: PublicShare;
  password?: string;
}) {
  const [requested, setRequested] = useState(data.has_password);
  const ticket = useShareTicket(code, password, requested);

  useEffect(() => {
    if (ticket.data) {
      window.location.href = buildTransferUrl(
        code,
        "download",
        ticket.data.ticket
      );
    }
  }, [ticket.data, code]);

  const going = ticket.isFetching || ticket.isSuccess;

  // After a failure the query is still enabled, so setting it again does not refetch.
  const handleGo = () => {
    if (requested) {
      ticket.refetch();
      return;
    }
    setRequested(true);
  };

  return (
    <div className="w-full max-w-sm animate-in fade-in duration-200">
      <Card className="w-full border-border/60 bg-card/60 shadow-2xl shadow-black/5 backdrop-blur-2xl">
        <CardHeader className="items-center text-center">
          <div className="mb-2 flex size-10 items-center justify-center rounded-full bg-muted text-muted-foreground">
            <Link2 className="size-5" />
          </div>
          <CardTitle className="break-words font-medium text-xl tracking-tight">
            {data.title || m.share_link_title()}
          </CardTitle>
          <CardDescription>{m.share_link_description()}</CardDescription>
          <ShareMeta
            expiresAt={data.expires_at}
            usesRemaining={data.uses_remaining}
          />
        </CardHeader>
        <CardContent>
          <Button
            className="w-full"
            disabled={going}
            onClick={handleGo}
            type="button"
          >
            <ExternalLink />
            {going ? m.share_link_redirecting() : m.share_link_go()}
          </Button>
        </CardContent>
      </Card>
    </div>
  );
}
