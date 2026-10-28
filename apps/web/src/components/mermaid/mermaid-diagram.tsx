import { Button } from "@tessera/ui/components/button";
import { Skeleton } from "@tessera/ui/components/skeleton";
import { cn } from "@tessera/ui/lib/utils";
import { Maximize2, TriangleAlert } from "lucide-react";
import { useState } from "react";

import { useMermaid } from "@/components/mermaid/use-mermaid";
import * as m from "@/paraglide/messages";

import { MermaidZoomDialog } from "./mermaid-zoom-dialog";

/** Redrawing on every keystroke is noisy, and the syntax is invalid mid-edit anyway. */
export const EDITOR_DEBOUNCE_MS = 400;

/**
 * Draws a mermaid diagram. A failed render shows the error and the source
 * instead of a blank space. `code` is not needed by the zoom dialog but is
 * shown as the source on error.
 */
export function MermaidDiagram({
  className,
  code,
  debounceMs = 0,
  title,
}: {
  className?: string;
  code: string;
  debounceMs?: number;
  title: string;
}) {
  const { svg, error, pending } = useMermaid(code, debounceMs);
  const [zoomed, setZoomed] = useState(false);

  if (pending && !svg) {
    return <Skeleton className={cn("h-48 w-full rounded-md", className)} />;
  }

  // Never rendered successfully: all we can show is the error and the source.
  if (!svg) {
    return (
      <div className={cn("flex flex-col gap-2", className)}>
        {error ? <ErrorBanner error={error} /> : null}
        <pre className="max-h-96 overflow-auto rounded-md border border-border/60 bg-muted/20 p-3 font-mono text-xs">
          {code}
        </pre>
      </div>
    );
  }

  return (
    <div className={cn("flex flex-col gap-2", className)}>
      {/* Rendered before but this parse failed: keep the previous diagram and
         mark the error above it. */}
      {error ? <ErrorBanner error={error} /> : null}
      <div className="group relative min-h-0 flex-1 overflow-hidden">
        <Button
          aria-label={m.mermaid_expand()}
          className="absolute top-2 right-2 z-10 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100"
          onClick={() => setZoomed(true)}
          size="icon-sm"
          type="button"
          variant="secondary"
        >
          <Maximize2 />
        </Button>
        {/* An SVG cannot go inside a <button> (nested interactive content is
            invalid), so zooming is triggered only by the top-right button and
            this layer is just a container. */}
        {/* useMermaid has set the SVG's width/height to its natural size, and
            the two max values clamp it: natural size if it fits, scaled down
            proportionally if not, with the zoom dialog for detail. Avoid
            w-full / h-auto: they enlarge small diagrams, and a tall flowchart
            stretched to full width gets far too high. absolute inset-0 gives
            max-h-full a definite height; percentage heights in a flex child do
            not always apply otherwise. */}
        <div
          className="absolute inset-0 flex items-center justify-center overflow-hidden rounded-md border border-border/60 bg-muted/20 p-4 [&_svg]:max-h-full [&_svg]:max-w-full"
          // oxlint-disable-next-line react/no-danger -- SVG produced by pele with links and images off
          dangerouslySetInnerHTML={{ __html: svg }}
        />
      </div>
      <MermaidZoomDialog
        onOpenChange={setZoomed}
        open={zoomed}
        svg={svg}
        title={title}
      />
    </div>
  );
}

function ErrorBanner({ error }: { error: string }) {
  return (
    <div className="flex gap-2 rounded-md border border-destructive/40 bg-destructive/5 p-3 text-left">
      <TriangleAlert className="mt-0.5 size-4 shrink-0 text-destructive" />
      <div className="min-w-0 flex-1">
        <p className="font-medium text-destructive text-xs">
          {m.mermaid_render_failed()}
        </p>
        <pre className="mt-1 overflow-x-auto whitespace-pre-wrap wrap-break-word font-mono text-[11px] text-muted-foreground">
          {error}
        </pre>
      </div>
    </div>
  );
}
