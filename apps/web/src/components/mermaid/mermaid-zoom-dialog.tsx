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

// 下限要夠低,不然超大的圖「置中縮到剛好」會卡在下限、還是塞不進視窗。
const MIN_SCALE = 0.02;
const MAX_SCALE = 8;
const ZOOM_STEP = 1.2;
const WHEEL_SENSITIVITY = 0.0015;
const PERCENT = 100;
/** 置中時圖跟視窗邊緣留的空隙。 */
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
        {/* 只在打開時掛載,關掉再開就回到原始比例,不用另外重設。 */}
        {open ? <ZoomViewer svg={svg} /> : null}
      </DialogContent>
    </Dialog>
  );
}

function clampScale(scale: number) {
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, scale));
}

/**
 * 以某個點為中心縮放:那個點在畫面上的位置要維持不變,
 * 所以平移量要跟著縮放比例一起補償。
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
 * 算出「整張圖剛好塞進視窗、並且置中」的 transform。
 * 圖比視窗小的時候不放大(scale 上限 1),免得小圖被撐糊。
 */
function fitTransform(
  viewport: HTMLElement,
  content: HTMLElement
): Transform | null {
  const svg = content.querySelector("svg");
  if (!svg) {
    return null;
  }
  // 量原始尺寸要先把目前的縮放拿掉,不然量到的是縮放後的結果。
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
  // 內容本來就用 flex 置中,所以縮放後只要把平移歸零就還是置中。
  return { scale, x: 0, y: 0 };
}

function ZoomViewer({ svg }: { svg: string }) {
  const [transform, setTransform] = useState<Transform>(IDENTITY);
  const viewportRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ x: number; y: number } | null>(null);

  /** 回到「整張圖看得見而且置中」的狀態。 */
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

  // 一打開就把整張圖縮到看得見,不然大圖會直接以 100% 開在左上角,
  // 使用者看到的是圖的一角(tessera#227)。
  useEffect(() => {
    const frame = requestAnimationFrame(fitToView);
    return () => cancelAnimationFrame(frame);
  }, [fitToView]);

  // React 的 onWheel 是被動監聽,preventDefault 會被忽略,
  // 所以要自己掛一個 passive: false 的原生監聽器擋掉頁面捲動。
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
        className="flex size-full items-center justify-center [&_svg]:h-auto! [&_svg]:max-w-none!"
        ref={contentRef}
        style={{
          transform: `translate(${transform.x}px, ${transform.y}px) scale(${transform.scale})`,
        }}
        // oxlint-disable-next-line react/no-danger -- SVG produced by mermaid with securityLevel "strict"
        dangerouslySetInnerHTML={{ __html: svg }}
      />
      {/* 這層一定要擋掉 pointerdown:否則會冒泡到外層觸發 setPointerCapture,
          事件被導去 viewport,按鈕收不到 pointerup 也就不會有 click。
          雙擊也要擋,不然按鈕連點兩下會觸發外層的 fit。 */}
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
