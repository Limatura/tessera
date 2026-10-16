import { Skeleton } from "@tessera/ui/components/skeleton";
import { useEffect, useState } from "react";

import { SvgImage } from "@/components/svg-image";
import * as m from "@/paraglide/messages";
import type { FilePreviewProps } from "@/routes/_site/s/$code/-components/file-previews/types";

/**
 * The download endpoint sends octet-stream + nosniff, so a browser will not draw it
 * as SVG and a plain <img src> shows a broken image. Fetch it first and let SvgImage
 * wrap it in an image/svg+xml blob.
 */
export function SvgPreview({ filename, url }: FilePreviewProps) {
  const [state, setState] = useState<
    | { status: "loading" }
    | { status: "error" }
    | { status: "ready"; blob: Blob }
  >({ status: "loading" });

  useEffect(() => {
    const controller = new AbortController();
    (async () => {
      try {
        const response = await fetch(url, { signal: controller.signal });
        if (!response.ok) {
          throw new Error(response.statusText);
        }
        setState({ blob: await response.blob(), status: "ready" });
      } catch {
        if (!controller.signal.aborted) {
          setState({ status: "error" });
        }
      }
    })();
    return () => controller.abort();
  }, [url]);

  if (state.status === "loading") {
    return <Skeleton className="h-48 w-full rounded-md" />;
  }

  if (state.status === "error") {
    return (
      <p className="rounded-md border border-border/60 bg-muted/30 p-6 text-center text-muted-foreground text-sm">
        {m.share_preview_failed()}
      </p>
    );
  }

  return <SvgImage alt={filename} className="max-h-96" source={state.blob} />;
}
