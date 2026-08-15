import { toast } from "@tessera/ui/components/toast";
import { useState } from "react";

import type { UploadResultData } from "@/lib/upload";

export type SubmitStatus = "idle" | "uploading" | "done";

/** 圓形展開特效的中心點和半徑,大小剛好蓋住整個視窗 */
interface Origin {
  x: number;
  y: number;
  radius: number;
}

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
 * 掌控所有上傳分頁共用的「送出」互動:按下按鈕後,`run` 實際打 API 並回報
 * 進度,完成後從該按鈕位置(見 `origin`)展開一個圓形特效來顯示結果頁面
 *
 * 狀態流: idle -> (start) -> uploading -> done -> expanded: true
 *        expanded: true -> (reset) -> expanded: false -> idle
 *        uploading -> (失敗) -> idle,並跳 toast
 */
export function useSubmitTransition() {
  const [status, setStatus] = useState<SubmitStatus>("idle");
  const [progress, setProgress] = useState(0);
  const [origin, setOrigin] = useState<Origin | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [result, setResult] = useState<UploadResultData | null>(null);

  // 傳入被點擊的按鈕元素(圓形展開特效從這裡開始)與實際執行上傳的函式
  // 如果已經在飛行中(status !== "idle"),呼叫會被忽略
  const start = async (
    el: HTMLElement | null,
    run: (onProgress: (percent: number) => void) => Promise<UploadResultData>
  ) => {
    if (status !== "idle" || !el) {
      return;
    }
    setOrigin(getExpandOrigin(el));
    setStatus("uploading");
    setProgress(0);

    try {
      const data = await run(setProgress);
      setResult(data);
      setStatus("done");
      setTimeout(() => setExpanded(true), 250);
    } catch (error) {
      setStatus("idle");
      setProgress(0);
      setOrigin(null);
      if (!(error instanceof DOMException && error.name === "AbortError")) {
        toast.add({
          description:
            error instanceof Error ? error.message : "Please try again.",
          title: "Upload failed",
          type: "error",
        });
      }
    }
  };

  // 先關掉展開效果(expanded: false 會播放圓形收合動畫)
  // 等動畫跑完(500ms)再清空其他狀態回到 idle
  const reset = () => {
    setExpanded(false);
    setTimeout(() => {
      setStatus("idle");
      setProgress(0);
      setOrigin(null);
      setResult(null);
    }, 500);
  };

  return { expanded, origin, progress, reset, result, start, status };
}
