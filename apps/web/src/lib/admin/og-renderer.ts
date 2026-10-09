import { fromHtml } from "@takumi-rs/helpers/html";
import init, { Renderer } from "@takumi-rs/wasm";
import wasmUrl from "@takumi-rs/wasm/takumi_wasm_bg.wasm?url";

import { CARD_HEIGHT, CARD_WIDTH } from "./og-card";

/**
 * 跟 `apps/server/src/og/mod.rs` 的 `FACES` 同名、同順序。順序就是 fallback
 * 順序:拉丁字先,中日文、韓文、emoji 依序補,等寬最後。少一個或順序錯,預覽的
 * 換行跟字形就會跟 server 畫出來的不一樣 —— 那正是 operator 打開這頁要確認的東西。
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
 * 載入 wasm 跟六個字型,整個分頁只做一次。約 3.8 MB wasm + 3.4 MB 字型,所以
 * 等到真的要畫才載。失敗就把 promise 清掉,下次呼叫能重試,不會永遠卡在同一個
 * rejected promise。
 */
export function loadCardRenderer(): Promise<Renderer> {
  ready ??= createRenderer();
  return ready;
}

async function createRenderer(): Promise<Renderer> {
  try {
    // 字型平行下載,但要照順序註冊 —— 註冊順序就是 fallback 順序。
    const [, faces] = await Promise.all([
      init({ module_or_path: wasmUrl }),
      Promise.all(FACES.map(fetchFace)),
    ]);
    const renderer = new Renderer();
    for (const face of faces) {
      // oxlint-disable-next-line no-await-in-loop -- 註冊順序就是 fallback 順序,不能平行
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
 * 把填好值的版面畫成 PNG。跟 server 一樣只給節點、尺寸跟字型,不給 stylesheet
 * 也不給圖片:版面規則本來就不允許 `<style>` 跟外部資源,多給只會讓預覽比實際好看。
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
