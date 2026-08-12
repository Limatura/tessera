import { useState } from "react";

export type SubmitStatus = "idle" | "uploading" | "done";

/** 圓形展開特效的中心點和半徑,大小剛好蓋住整個視窗 */
interface Origin {
  x: number;
  y: number;
  radius: number;
}

const UPLOAD_DURATION_MS = 1400;

/** 計算從按鈕中心展開、剛好蓋滿整個視窗的圓 */
function getExpandOrigin(el: HTMLElement): Origin {
  const rect = el.getBoundingClientRect();
  const x = rect.left + rect.width / 2;
  const y = rect.top + rect.height / 2;
  const radius = Math.hypot(
    Math.max(x, window.innerWidth - x),
    Math.max(y, window.innerHeight - y)
  );
  return { radius, x, y };
}

/**
 * 掌控所有上傳分頁共用的「送出」互動:按下按鈕會跑一個假進度條(共 UPLOAD_DURATION_MS ms),
 * 然後從該按鈕位置(見 `origin`)展開一個圓形特效來顯示結果頁面
 * 目前還沒真正上傳 — `start()` 只是做進度條動畫,等 `lib/upload.ts` 的 API 完成後
 * 再在這裡呼叫它就行
 *
 * 狀態流: idle -> (start) -> uploading -> done -> expanded: true
 *        expanded: true -> (reset) -> expanded: false -> idle
 */
export function useSubmitTransition() {
  const [status, setStatus] = useState<SubmitStatus>("idle");
  const [progress, setProgress] = useState(0);
  const [origin, setOrigin] = useState<Origin | null>(null);
  const [expanded, setExpanded] = useState(false);

  // 傳入被點擊的按鈕元素(圓形展開特效從這裡開始)
  // 如果已經在飛行中(status !== "idle"),呼叫會被忽略
  const start = (el: HTMLElement | null) => {
    if (status !== "idle" || !el) {
      return;
    }
    setOrigin(getExpandOrigin(el));
    setStatus("uploading");
    setProgress(0);

    const startTime = performance.now();
    // 每一幀推進 `progress` 直到 100%,接著切到 "done" 狀態
    // 延遲 250ms 後才設定 expanded: true 觸發圓形展開動畫
    const tick = (now: number) => {
      const elapsed = now - startTime;
      const next = Math.min(100, (elapsed / UPLOAD_DURATION_MS) * 100);
      setProgress(next);
      if (next < 100) {
        requestAnimationFrame(tick);
      } else {
        setStatus("done");
        setTimeout(() => setExpanded(true), 250);
      }
    };
    requestAnimationFrame(tick);
  };

  // 先關掉展開效果(expanded: false 會播放圓形收合動畫)
  // 等動畫跑完(500ms)再清空其他狀態回到 idle
  const reset = () => {
    setExpanded(false);
    setTimeout(() => {
      setStatus("idle");
      setProgress(0);
      setOrigin(null);
    }, 500);
  };

  return { expanded, origin, progress, reset, start, status };
}
