import { useTheme } from "@tessera/ui/components/theme-provider";
import { useEffect, useRef, useState } from "react";

/**
 * The render result for one diagram. `svg` is the last successful output and
 * is kept when a later parse fails, so a syntax error mid-edit does not blank
 * the view.
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
 * When mermaid.render() fails it leaves its measuring container on body, and
 * these pile up. See mermaid-js/mermaid#4730.
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
 * mermaid emits `width="100%" style="max-width: Npx"`: the width follows the
 * container and the natural size hides in the inline style. Writing the
 * viewBox's width and height into width/height lets the display use
 * max-w-full + max-h-full (natural size if it fits, scaled down if not), and
 * lets the zoom dialog measure the natural size. The viewBox defaults to
 * xMidYMid meet, so when the two max values give a different aspect ratio the
 * diagram is centred with blank space instead of being distorted.
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
 * Renders mermaid source to an SVG string. mermaid is large (~500 KB gzip), so
 * it is loaded dynamically only when a diagram is actually drawn, keeping it
 * out of the main bundle.
 */
export function useMermaid(code: string, debounceMs = 0): MermaidState {
  const { resolvedTheme } = useTheme();
  // renderedFor records which source produced this result. pending is derived
  // from it, so the effect does not setState synchronously and cascade renders.
  const [result, setResult] = useState<{
    error: string | null;
    renderedFor: string | null;
    svg: string | null;
  }>({ error: null, renderedFor: null, svg: null });
  // Keep the last good diagram so it does not vanish while the syntax is invalid mid-edit.
  const lastSvg = useRef<string | null>(null);

  useEffect(() => {
    const trimmed = code.trim();
    // Empty input has nothing to draw. Its early return is derived below, so
    // state is untouched here.
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
            // Pasted content is untrusted: strict blocks click interaction
            // directives and keeps HTML in labels out of the output SVG.
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
