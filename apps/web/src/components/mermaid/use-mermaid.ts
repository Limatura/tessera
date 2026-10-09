import { useTheme } from "@tessera/ui/components/theme-provider";
import { useEffect, useRef, useState } from "react";

/**
 * 一張圖的渲染結果。`svg` 是上一次成功的輸出,即使這次解析失敗也留著,
 * 讓編輯時的中途語法錯誤不會把畫面清空(tessera#227)。
 */
export interface MermaidState {
  svg: string | null;
  error: string | null;
  pending: boolean;
}

let idCounter = 0;

function nextId() {
  idCounter += 1;
  return `mermaid-${idCounter}`;
}

function messageOf(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return typeof error === "string" ? error : "Failed to render diagram";
}

/**
 * mermaid.render() 失敗時會把量測用的容器留在 body 上,不清會越積越多。
 * 見 mermaid-js/mermaid#4730。
 */
function removeOrphanContainer(id: string) {
  for (const selector of [`#d${id}`, `#${id}`]) {
    document.querySelector(selector)?.remove();
  }
}

const SVG_OPEN_TAG = /<svg\b[^>]*>/u;
const VIEW_BOX_SIZE =
  /\sviewBox="[^\s"]+\s+[^\s"]+\s+(?<width>[^\s"]+)\s+(?<height>[^\s"]+)"/u;
const SIZE_ATTRIBUTE = /\s(?:width|height)="[^"]*"/gu;
const MAX_WIDTH_STYLE = /max-width:\s*[^;"]*;?/u;

/**
 * mermaid 輸出 `width="100%" style="max-width: Npx"`:寬度跟著容器走,
 * 原尺寸只藏在 inline style 裡。改成 viewBox 的原始寬高寫進 width/height,
 * 顯示端用 max-w-full + max-h-full 就能「塞得下就原尺寸、塞不下就等比縮小」,
 * 放大 dialog 量到的也才是原尺寸。viewBox 預設 xMidYMid meet,
 * 框被兩個 max 夾成不同比例時圖只會置中留白,不會變形。
 */
function withNaturalSize(svg: string): string {
  const openTag = SVG_OPEN_TAG.exec(svg)?.[0];
  const size = openTag ? VIEW_BOX_SIZE.exec(openTag)?.groups : undefined;
  if (!(openTag && size)) {
    return svg;
  }
  const { width, height } = size;
  const sized = openTag
    .replace(SIZE_ATTRIBUTE, "")
    .replace(MAX_WIDTH_STYLE, "")
    .replace("<svg", `<svg width="${width}" height="${height}"`);
  return svg.replace(openTag, () => sized);
}

/**
 * 把 mermaid 原始碼渲染成 SVG 字串。mermaid 這包很大(~500KB gzip),
 * 所以只在真的要畫圖時才動態載入,不進主 bundle。
 */
export function useMermaid(code: string, debounceMs = 0): MermaidState {
  const { resolvedTheme } = useTheme();
  // renderedFor 記錄這份結果是哪段原始碼算出來的,用來推導 pending,
  // 免得在 effect 裡同步 setState 觸發連鎖 render。
  const [result, setResult] = useState<{
    error: string | null;
    renderedFor: string | null;
    svg: string | null;
  }>({ error: null, renderedFor: null, svg: null });
  // 保留上一張成功的圖,打字打到一半語法不合法時不要整塊消失。
  const lastSvg = useRef<string | null>(null);

  useEffect(() => {
    const trimmed = code.trim();
    // 空內容沒東西可畫,直接回傳的分支在下面推導,這裡不用動 state。
    if (trimmed.length === 0) {
      lastSvg.current = null;
      return;
    }

    let cancelled = false;

    const timer = setTimeout(() => {
      (async () => {
        const id = nextId();
        try {
          const { default: mermaid } = await import("mermaid");
          mermaid.initialize({
            // 使用者貼進來的內容不可信:strict 會擋掉 click 互動指令,
            // 並且不讓 label 裡的 HTML 進到輸出的 SVG。
            securityLevel: "strict",
            startOnLoad: false,
            suppressErrorRendering: true,
            theme: resolvedTheme === "dark" ? "dark" : "default",
          });
          const rendered = await mermaid.render(id, trimmed);
          if (cancelled) {
            return;
          }
          const svg = withNaturalSize(rendered.svg);
          lastSvg.current = svg;
          setResult({ error: null, renderedFor: code, svg });
        } catch (error) {
          removeOrphanContainer(id);
          if (cancelled) {
            return;
          }
          setResult({
            error: messageOf(error),
            renderedFor: code,
            svg: lastSvg.current,
          });
        }
      })();
    }, debounceMs);

    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [code, resolvedTheme, debounceMs]);

  if (code.trim().length === 0) {
    return { error: null, pending: false, svg: null };
  }

  return {
    error: result.error,
    pending: result.renderedFor !== code,
    svg: result.svg,
  };
}
