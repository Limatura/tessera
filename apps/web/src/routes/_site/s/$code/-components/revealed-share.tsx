import type { components } from "@tessera/api/schema";
import { Button } from "@tessera/ui/components/button";
import { Skeleton } from "@tessera/ui/components/skeleton";
import { Download, Eye, FileIcon, Flame } from "lucide-react";
import { useEffect, useState } from "react";

import { useJoinedDownload } from "@/hooks/use-joined-download";
import { useShareTicket } from "@/hooks/use-share";
import { buildTransferUrl } from "@/lib/api/share";
import * as m from "@/paraglide/messages";

import { resolveFilePreview } from "./file-previews/registry";
import type { FilePreviewComponent } from "./file-previews/types";
import { TextPreview } from "./text-preview";

type PublicShare = components["schemas"]["PublicShareResponse"];

/**
 * What the revealed card should render for this share. Resolved once,
 * synchronously, from metadata already on hand, so deciding whether to
 * auto-fetch a preview needs no network round trip.
 */
type PreviewPlan =
  | { type: "text" }
  | { type: "file"; Component: FilePreviewComponent }
  | { type: "none" };

/**
 * This is the last retrieval left (burn after reading, or one remaining).
 *
 * A preview needs a ticket, and a ticket spends a retrieval: auto-previewing would use
 * it up as the page opens, and a previewed image can be saved with a right click, so
 * "download once" would mean nothing. These shares wait for an explicit click.
 */
function isLastRetrieval(data: PublicShare): boolean {
  return data.uses_remaining === 1;
}

function resolvePreviewPlan(data: PublicShare): PreviewPlan {
  if (data.kind === "text") {
    return { type: "text" };
  }
  // A multi-file share downloads as an archive. Previewing its first file would
  // suggest that file is what was shared.
  if (isLastRetrieval(data) || data.files.length > 1) {
    return { type: "none" };
  }
  const filename = data.files[0]?.filename;
  const Component = filename ? resolveFilePreview(filename) : null;
  return Component ? { Component, type: "file" } : { type: "none" };
}

export function RevealedShare({
  code,
  data,
  password,
}: {
  code: string;
  data: PublicShare;
  password?: string;
}) {
  const plan = resolvePreviewPlan(data);
  const oneTime = isLastRetrieval(data);
  const [manualRequested, setManualRequested] = useState(false);
  const ticket = useShareTicket(
    code,
    password,
    (plan.type !== "none" && !oneTime) || manualRequested
  );
  // One-time text fetches its ticket only on "View text", with no download button
  // before that: downloading would spend the one retrieval and show nothing.
  const textHidden = plan.type === "text" && oneTime && !manualRequested;
  const joined = useJoinedDownload(code, data);

  useEffect(() => {
    if (plan.type === "none" && manualRequested && ticket.data) {
      window.location.href = buildTransferUrl(
        code,
        "download",
        ticket.data.ticket
      );
    }
  }, [plan.type, manualRequested, ticket.data, code]);

  const handleDownloadClick = async () => {
    if (joined.available && (await joined.start())) {
      return;
    }
    if (ticket.data) {
      window.location.href = buildTransferUrl(
        code,
        "download",
        ticket.data.ticket
      );
      return;
    }
    setManualRequested(true);
  };

  return (
    <div className="flex flex-col gap-4">
      {textHidden && (
        <div className="flex flex-col items-center gap-3 rounded-md border border-border/60 bg-muted/20 p-8 text-center">
          <Flame className="size-8 text-muted-foreground" />
          <p className="text-muted-foreground text-sm">
            {m.share_one_time_text()}
          </p>
          <Button onClick={() => setManualRequested(true)} type="button">
            <Eye />
            {m.share_view_text()}
          </Button>
        </div>
      )}

      {plan.type === "text" &&
        !textHidden &&
        (ticket.data ? (
          <TextPreview
            language={data.language}
            sizeBytes={data.size_bytes}
            url={buildTransferUrl(code, "raw", ticket.data.ticket)}
          />
        ) : (
          <Skeleton className="h-48 w-full rounded-md" />
        ))}

      {plan.type === "file" &&
        (ticket.data ? (
          <plan.Component
            filename={data.files[0]?.filename ?? ""}
            url={buildTransferUrl(code, "download", ticket.data.ticket)}
          />
        ) : (
          <Skeleton className="h-48 w-full rounded-md" />
        ))}

      {plan.type === "none" && (
        <div className="flex flex-col items-center gap-2 rounded-md border border-border/60 bg-muted/20 p-8 text-center">
          {oneTime ? (
            <Flame className="size-8 text-muted-foreground" />
          ) : (
            <FileIcon className="size-8 text-muted-foreground" />
          )}
          <p className="text-muted-foreground text-sm">
            {oneTime ? m.share_one_time_file() : m.share_no_preview()}
          </p>
        </div>
      )}

      {!textHidden && (
        <Button
          className="w-full"
          disabled={ticket.isFetching || joined.joining}
          onClick={handleDownloadClick}
          type="button"
        >
          <Download />
          {downloadLabel(ticket.isFetching, joined.joining)}
        </Button>
      )}

      {joined.joining && (
        <p className="text-center text-muted-foreground text-xs">
          {m.share_join_keep_open()}
        </p>
      )}
    </div>
  );
}

function downloadLabel(preparing: boolean, joining: boolean) {
  if (joining) {
    return m.share_joining();
  }
  return preparing ? m.share_preparing() : m.share_download();
}
