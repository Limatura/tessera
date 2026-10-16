import { cn } from "@tessera/ui/lib/utils";
import { useEffect, useRef, useState } from "react";

import * as m from "@/paraglide/messages";

/**
 * Displays a user-supplied SVG.
 *
 * It must be wrapped in an image/svg+xml blob and shown in an <img>: SVG in an
 * <img> runs no script, which makes it the only safe way to display it. Do not
 * switch to dangerouslySetInnerHTML, <object>, <iframe> or <embed>; they all
 * run the SVG's <script>, making it stored XSS. The download endpoint answers
 * octet-stream + nosniff by design, so its URL cannot be the src either.
 */
export function SvgImage({
  alt,
  className,
  source,
}: {
  alt: string;
  className?: string;
  /** SVG source text or bytes. */
  source: string | Blob;
}) {
  const imgRef = useRef<HTMLImageElement>(null);
  // Remember which source failed to render, so new content (while editing) retries automatically.
  const [failedSource, setFailedSource] = useState<string | Blob | null>(null);
  const failed = failedSource === source;

  // A blob URL is an external resource: create it in the effect, revoke it on
  // cleanup, and put it on the <img>.
  useEffect(() => {
    const img = imgRef.current;
    if (!img) {
      return;
    }
    const objectUrl = URL.createObjectURL(
      new Blob([source], { type: "image/svg+xml" })
    );
    img.src = objectUrl;
    return () => URL.revokeObjectURL(objectUrl);
  }, [source]);

  if (failed) {
    return (
      <p
        className={cn(
          "rounded-md border border-border/60 bg-muted/30 p-6 text-center text-muted-foreground text-sm",
          className
        )}
      >
        {m.share_preview_failed()}
      </p>
    );
  }

  return (
    <img
      alt={alt}
      className={cn(
        "w-full rounded-md border border-border/60 bg-muted/20 object-contain p-4",
        className
      )}
      onError={() => setFailedSource(source)}
      ref={imgRef}
    />
  );
}
