import { fromHtml } from "@takumi-rs/helpers/html";
import init, { Renderer } from "@takumi-rs/wasm";
import wasmUrl from "@takumi-rs/wasm/takumi_wasm_bg.wasm?url";

import { CARD_HEIGHT, CARD_WIDTH } from "./og-card";

/**
 * Same names and order as `FACES` in `apps/server/src/og/mod.rs`. The order is
 * the fallback order: Latin first, then CJK, Korean and emoji, monospace last.
 * A missing face or a different order makes the preview's line breaks and
 * glyphs differ from what the server draws, and that is what the operator opens
 * this page to check.
 */
const FACES = [
  "DejaVuSans.woff2",
  "DejaVuSans-Bold.woff2",
  "DroidSansFallback.woff2",
  "NotoSansKR-hangul.woff2",
  "NotoEmoji.woff2",
  "DejaVuSansMono.woff2",
] as const;

let ready: Promise<Renderer> | undefined;

/**
 * Loads the wasm and the six fonts once per tab. That is about 3.8 MB of wasm
 * plus 3.4 MB of fonts, so it waits until something is drawn. On failure the
 * promise is cleared so the next call can retry instead of reusing a rejected
 * promise forever.
 */
export function loadCardRenderer(): Promise<Renderer> {
  ready ??= createRenderer();
  return ready;
}

async function createRenderer(): Promise<Renderer> {
  try {
    // Fonts download in parallel but register in order: registration order is fallback order.
    const [, faces] = await Promise.all([
      init({ module_or_path: wasmUrl }),
      Promise.all(FACES.map(fetchFace)),
    ]);
    const renderer = new Renderer();
    for (const face of faces) {
      // oxlint-disable-next-line no-await-in-loop -- registration order is fallback order, so it cannot be parallel
      await renderer.registerFont(face);
    }
    return renderer;
  } catch (error) {
    ready = undefined;
    throw error;
  }
}

async function fetchFace(name: string): Promise<Uint8Array> {
  const response = await fetch(`/fonts/${name}`);
  if (!response.ok) {
    throw new Error(`/fonts/${name}: ${response.status}`);
  }
  return new Uint8Array(await response.arrayBuffer());
}

/**
 * Draws the filled layout as a PNG. As on the server, only nodes, size and
 * fonts are given, no stylesheet or images: layout rules forbid `<style>` and
 * external resources, so giving more would only make the preview look better
 * than the real card.
 */
export async function renderCard(markup: string): Promise<Blob> {
  const renderer = await loadCardRenderer();
  const { node } = fromHtml(markup);
  const png = await renderer.render(node, {
    height: CARD_HEIGHT,
    width: CARD_WIDTH,
  });
  return new Blob([png], { type: "image/png" });
}
