import { Button } from "@tessera/ui/components/button";
import { Skeleton } from "@tessera/ui/components/skeleton";
import { cn } from "@tessera/ui/lib/utils";
import { Maximize2, TriangleAlert } from "lucide-react";
import { useState } from "react";

import { useMermaid } from "@/components/mermaid/use-mermaid";
import * as m from "@/paraglide/messages";

import { MermaidZoomDialog } from "./mermaid-zoom-dialog";

/** 編輯時每打一個字都重畫太吵,而且中途語法必然不合法。 */
export const EDITOR_DEBOUNCE_MS = 400;

function ErrorBanner({ error }: { error: string }) {
  return (
    <div className="flex gap-2 rounded-md border border-destructive/40 bg-destructive/5 p-3 text-left">
      <TriangleAlert className="mt-0.5 size-4 shrink-0 text-destructive" />
      <div className="min-w-0 flex-1">
        <p className="font-medium text-destructive text-xs">
          {m.mermaid_render_failed()}
        </p>
        <pre className="mt-1 overflow-x-auto whitespace-pre-wrap break-words font-mono text-[11px] text-muted-foreground">
          {error}
        </pre>
      </div>
    </div>
  );
}

/**
 * 畫一張 mermaid 圖。渲染失敗時顯示錯誤訊息加原始碼,不留白(tessera#227)。
 * `code` 會傳給放大 dialog 當標題用不到,但錯誤時要拿來顯示原始碼。
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

  // 從沒畫成功過:只能顯示錯誤加原始碼。
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
      {/* 曾經畫成功過但這次解析失敗:留著上一張圖,只在上面標一行錯誤。 */}
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
        {/* SVG 不能放進 <button>(巢狀互動元素不合法),所以放大只靠右上角
            那顆按鈕觸發,這層純粹是容器。 */}
        {/* mermaid 會在 <svg> 上寫死 style="max-width: <圖寬>px",inline style
            贏過 class,所以複雜的大圖會把卡片撐爆。用 [&_svg]:max-w-full! 的
            !important 蓋掉它,再讓 SVG 自己縮到容器寬度(tessera#227)。 */}
        <div
          className="flex max-h-full w-full items-center justify-center overflow-auto rounded-md border border-border/60 bg-muted/20 p-4 [&_svg]:h-auto! [&_svg]:max-w-full! [&_svg]:w-full"
          // oxlint-disable-next-line react/no-danger -- SVG produced by mermaid with securityLevel "strict"
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
