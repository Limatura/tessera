import { createFileRoute } from "@tanstack/react-router";

import { UploadPanel } from "./-components/upload-panel";

export const Route = createFileRoute("/_site/")({
  component: HomeComponent,
});

function HomeComponent() {
  // min-w-0:這層是 __root 那個 grid 的 item,預設 min-width:auto 會讓它被
  // 內容(貼進 CodeMirror 的長行)往外推,連帶把整張卡片撐出視窗。
  // 不用 items-center:卡片比視窗高時(手機)會往上下對稱溢出,上半截滾不到。
  // 由 UploadPanel 的外層用 m-auto 置中,夠高就置中、太高就從頂端往下排並可捲動。
  return (
    <div className="relative z-0 flex min-h-svh w-full min-w-0 px-6 py-20">
      <UploadPanel />
    </div>
  );
}
