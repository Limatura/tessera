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

function messageOf(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return typeof error === "string" ? error : "Failed to render diagram";
}

/**
 * Renders mermaid source to an SVG string with pele. It is loaded dynamically
 * only when a diagram is actually drawn, keeping it out of the main bundle.
 * The SVG carries no theme: its colours are CSS variables bound to the design
 * tokens in index.css, so a theme change needs no re-render.
 */
export function useMermaid(code: string, debounceMs = 0): MermaidState {
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
        try {
          const { renderAsync } = await import("pele/lazy");
          // Pasted content is untrusted: no links or images in the output.
          const rendered = await renderAsync(trimmed, {
            images: false,
            links: false,
          });
          if (cancelled) {
            return;
          }
          lastSvg.current = rendered.svg;
          setResult({ error: null, renderedFor: code, svg: rendered.svg });
        } catch (error) {
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
  }, [code, debounceMs]);

  if (code.trim().length === 0) {
    return { error: null, pending: false, svg: null };
  }

  return {
    error: result.error,
    pending: result.renderedFor !== code,
    svg: result.svg,
  };
}
