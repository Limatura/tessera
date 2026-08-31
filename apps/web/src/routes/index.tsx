import { createRoute } from "@tanstack/react-router";

import { UploadPanel } from "./-components/upload-panel";
import { Route as rootRoute } from "./__root";

export const Route = createRoute({
  component: HomeComponent,
  getParentRoute: () => rootRoute,
  path: "/",
});

function HomeComponent() {
  // min-w-0:這層是 __root 那個 grid 的 item,預設 min-width:auto 會讓它被
  // 內容(貼進 CodeMirror 的長行)往外推,連帶把整張卡片撐出視窗。
  return (
    <div className="relative z-0 flex min-h-svh w-full min-w-0 items-center justify-center p-6">
      <UploadPanel />
    </div>
  );
}
