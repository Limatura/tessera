import { cn } from "@tessera/ui/lib/utils";
import { useEffect, useRef, useState } from "react";

import * as m from "@/paraglide/messages";

/**
 * 顯示使用者提供的 SVG(tessera#467、#468)。
 *
 * 一定要包成 image/svg+xml 的 blob 再放進 <img>:<img> 裡的 SVG 不會執行
 * script,這是唯一安全的顯示方式。不要改用 dangerouslySetInnerHTML、<object>、
 * <iframe>、<embed>——那些都會跑 SVG 裡的 <script>,等於 stored XSS。
 * 後端下載端點故意回 octet-stream + nosniff,所以也不能直接拿下載網址當 src。
 */
export function SvgImage({
  alt,
  className,
  source,
}: {
  alt: string;
  className?: string;
  /** SVG 原始碼或位元組。 */
  source: string | Blob;
}) {
  const imgRef = useRef<HTMLImageElement>(null);
  // 記下哪一份 source 畫失敗,換了內容(編輯中)就自動重試。
  const [failedSource, setFailedSource] = useState<string | Blob | null>(null);
  const failed = failedSource === source;

  // blob URL 是外部資源:effect 裡建、cleanup 時 revoke,直接寫進 <img>。
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
