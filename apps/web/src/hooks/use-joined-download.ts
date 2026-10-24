import { useQuery } from "@tanstack/react-query";
import type { components } from "@tessera/api/schema";
import { toast } from "@tessera/ui/components/toast";
import { useState } from "react";

import { canSaveStreams, openSaveTarget } from "@/lib/download/save-stream";
import {
  fetchSegment,
  fetchSegmentLayout,
  joinSegments,
} from "@/lib/download/segments";
import * as m from "@/paraglide/messages";

type PublicShare = components["schemas"]["PublicShareResponse"];

/**
 * Whether the server could hand this share's pieces out directly at all. It
 * answers `404` otherwise, but asking only when it might say yes saves a
 * request on every password-protected or limited share.
 */
function mightJoin(data: PublicShare): boolean {
  return (
    data.kind === "file" &&
    !data.has_password &&
    (data.uses_remaining === null || data.uses_remaining === undefined) &&
    data.files.length === 1 &&
    canSaveStreams()
  );
}

/**
 * Downloads a split file by fetching its pieces from the stores and joining
 * them here. Only for a file in more than one piece: one in a single
 * piece is better served by `/download`, which the server can redirect to the
 * store itself and the browser downloads natively, with no tab to keep open.
 *
 * `start` resolves `false` when the joined download could not begin and the
 * caller should use `/download` instead.
 */
export function useJoinedDownload(code: string, data: PublicShare) {
  const eligible = mightJoin(data);
  const layout = useQuery({
    enabled: eligible,
    // Describing the layout spends nothing, so it is safe to ask on page load
    queryFn: () => fetchSegmentLayout(code),
    queryKey: ["share-segments", code],
    retry: false,
    staleTime: Number.POSITIVE_INFINITY,
  });
  const [joining, setJoining] = useState(false);

  const available =
    eligible && (layout.data?.segments.length ?? 0) > 1 ? layout.data : null;

  // Must be called straight from the click: the save picker needs the gesture.
  const start = async (): Promise<boolean> => {
    if (!available) {
      return false;
    }
    // A file from a folder upload is stored as `folder/file`, and the save
    // picker refuses a name with a separator in it
    const name = data.files[0]?.filename?.split("/").at(-1) || code;
    let target: Awaited<ReturnType<typeof openSaveTarget>>;
    try {
      target = await openSaveTarget(name, available.size_bytes);
    } catch {
      // No picker and the worker would not register: /download still works
      return false;
    }
    if (!target) {
      // The visitor dismissed the picker: they did not want the file after all
      return true;
    }

    let first: Response;
    try {
      first = await fetchSegment(code, 0);
    } catch {
      // Most often a bucket without CORS for this site: /download still works
      target.cancel();
      return false;
    }

    setJoining(true);
    try {
      await target.write(joinSegments(code, available, first));
    } catch (error) {
      if (!(error instanceof DOMException && error.name === "AbortError")) {
        toast.add({
          description: m.share_join_failed_description(),
          title: m.share_join_failed_title(),
          type: "error",
        });
      }
    } finally {
      setJoining(false);
    }
    return true;
  };

  return { available: available !== null, joining, start };
}
