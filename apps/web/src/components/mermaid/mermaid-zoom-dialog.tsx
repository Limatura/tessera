import { Button } from "@tessera/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@tessera/ui/components/dialog";
import { Minus, Plus, RotateCcw } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import * as m from "@/paraglide/messages";

// Low enough that fitting a huge diagram is not stopped by the minimum before it fits the window.
const MIN_SCALE = 0.02;
const MAX_SCALE = 8;
const ZOOM_STEP = 1.2;
const WHEEL_SENSITIVITY = 0.0015;
const PERCENT = 100;
/** Gap between the diagram and the window edge when fitted. */
const FIT_PADDING = 24;

interface Transform {
  scale: number;
  x: number;
  y: number;
}

const IDENTITY: Transform = { scale: 1, x: 0, y: 0 };

export function MermaidZoomDialog({
  onOpenChange,
  open,
  svg,
  title,
}: {
  onOpenChange: (open: boolean) => void;
  open: boolean;
  svg: string;
  title: string;
}) {
  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="flex h-[92vh] w-[95vw] max-w-6xl flex-col gap-2 sm:max-w-6xl">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        {/* Mounted only while open, so reopening starts at the original scale without a reset. */}
        {open ? <ZoomViewer svg={svg} /> : null}
      </DialogContent>
    </Dialog>
  );
}

function clampScale(scale: number) {
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, scale));
}

/**
 * Zooms around a point. That point must stay at the same place on screen, so
 * the translation is compensated along with the scale.
 */
function zoomAround(
  current: Transform,
  nextScale: number,
  originX: number,
  originY: number
): Transform {
  const scale = clampScale(nextScale);
  const ratio = scale / current.scale;
  return {
    scale,
    x: originX - (originX - current.x) * ratio,
    y: originY - (originY - current.y) * ratio,
  };
}

/**
 * The transform that fits the entire diagram in the window, centred. A diagram
 * smaller than the window is not enlarged (scale is capped at 1), so it does
 * not blur.
 */
function fitTransform(
  viewport: HTMLElement,
  content: HTMLElement
): Transform | null {
  const svg = content.querySelector("svg");
  if (!svg) {
    return null;
  }
  // Remove the current scale before measuring, or the measurement is the scaled size.
  const previous = content.style.transform;
  content.style.transform = "none";
  const box = svg.getBoundingClientRect();
  content.style.transform = previous;

  if (box.width === 0 || box.height === 0) {
    return null;
  }

  const available = viewport.getBoundingClientRect();
  const scale = clampScale(
    Math.min(
      1,
      (available.width - FIT_PADDING * 2) / box.width,
      (available.height - FIT_PADDING * 2) / box.height
    )
  );
  // The content is centred with flex, so zeroing the translation keeps it centred after scaling.
  return { scale, x: 0, y: 0 };
}

function ZoomViewer({ svg }: { svg: string }) {
  const [transform, setTransform] = useState<Transform>(IDENTITY);
  const viewportRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ x: number; y: number } | null>(null);

  /** Back to the entire diagram visible and centred. */
  const fitToView = useCallback(() => {
    const viewport = viewportRef.current;
    const content = contentRef.current;
    if (!(viewport && content)) {
      return;
    }
    const next = fitTransform(viewport, content);
    setTransform(next ?? IDENTITY);
  }, []);

  const zoomBy = useCallback((factor: number) => {
    const rect = viewportRef.current?.getBoundingClientRect();
    const originX = rect ? rect.width / 2 : 0;
    const originY = rect ? rect.height / 2 : 0;
    setTransform((current) =>
      zoomAround(current, current.scale * factor, originX, originY)
    );
  }, []);

  // Fit the diagram on open. Otherwise a large one opens at 100% in the
  // top-left corner and the user sees only a corner of it.
  useEffect(() => {
    const frame = requestAnimationFrame(fitToView);
    return () => cancelAnimationFrame(frame);
  }, [fitToView]);

  // React's onWheel listener is passive and ignores preventDefault, so a native
  // listener with passive: false is attached to stop the page scrolling.
  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) {
      return;
    }

    const handleWheel = (event: WheelEvent) => {
      event.preventDefault();
      const rect = viewport.getBoundingClientRect();
      setTransform((current) =>
        zoomAround(
          current,
          current.scale * Math.exp(-event.deltaY * WHEEL_SENSITIVITY),
          event.clientX - rect.left,
          event.clientY - rect.top
        )
      );
    };

    viewport.addEventListener("wheel", handleWheel, { passive: false });
    return () => viewport.removeEventListener("wheel", handleWheel);
  }, []);

  const handlePointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    dragRef.current = {
      x: event.clientX - transform.x,
      y: event.clientY - transform.y,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const handlePointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const origin = dragRef.current;
    if (!origin) {
      return;
    }
    setTransform((current) => ({
      ...current,
      x: event.clientX - origin.x,
      y: event.clientY - origin.y,
    }));
  };

  const handlePointerUp = (event: React.PointerEvent<HTMLDivElement>) => {
    dragRef.current = null;
    event.currentTarget.releasePointerCapture(event.pointerId);
  };

  return (
    <div
      className="relative flex-1 cursor-grab overflow-hidden rounded-md border border-border/60 bg-muted/20 active:cursor-grabbing"
      onDoubleClick={fitToView}
      onPointerCancel={handlePointerUp}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      ref={viewportRef}
    >
      <div
        className="flex size-full items-center justify-center [&_svg]:max-w-none [&_svg]:shrink-0"
        ref={contentRef}
        style={{
          transform: `translate(${transform.x}px, ${transform.y}px) scale(${transform.scale})`,
        }}
        // oxlint-disable-next-line react/no-danger -- SVG produced by pele with links and images off
        dangerouslySetInnerHTML={{ __html: svg }}
      />
      {/* This layer must stop pointerdown. Otherwise it bubbles to the outer
          layer, which calls setPointerCapture; events then go to the viewport,
          the buttons never get pointerup and so never click. Double-click is
          stopped too, or double-clicking a button would trigger the outer fit. */}
      <div
        className="absolute right-3 bottom-3 flex items-center gap-1 rounded-md border border-border/60 bg-background/90 p-1 shadow-lg backdrop-blur"
        onDoubleClick={(event) => event.stopPropagation()}
        onPointerDown={(event) => event.stopPropagation()}
      >
        <Button
          aria-label={m.mermaid_zoom_out()}
          onClick={() => zoomBy(1 / ZOOM_STEP)}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          <Minus />
        </Button>
        <span className="min-w-12 text-center font-mono text-muted-foreground text-xs tabular-nums">
          {Math.round(transform.scale * PERCENT)}%
        </span>
        <Button
          aria-label={m.mermaid_zoom_in()}
          onClick={() => zoomBy(ZOOM_STEP)}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          <Plus />
        </Button>
        <Button
          aria-label={m.mermaid_reset_zoom()}
          onClick={fitToView}
          size="icon-sm"
          title={m.mermaid_reset_zoom()}
          type="button"
          variant="ghost"
        >
          <RotateCcw />
        </Button>
      </div>
    </div>
  );
}
