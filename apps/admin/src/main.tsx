import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider, createRouter } from "@tanstack/react-router";
import { Loader } from "@tessera/ui/components/loader";
import ReactDOM from "react-dom/client";

import { applySiteAppearance } from "./lib/appearance";
import { routeTree } from "./routeTree.gen";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // 後台資料改動頻繁，切回分頁時重抓比看到舊的使用者/角色列表安全。
      refetchOnWindowFocus: true,
      staleTime: 30_000,
    },
  },
});

const router = createRouter({
  context: { queryClient },
  defaultPendingComponent: () => <Loader />,
  defaultPreload: "intent",
  routeTree,
  scrollRestoration: true,
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

const rootElement = document.querySelector("#app");

if (!rootElement) {
  throw new Error("Root element not found");
}

if (!rootElement.innerHTML) {
  const root = ReactDOM.createRoot(rootElement);

  /*
   * 先上色再掛載。後台拿不到 `assets.rs` 寫進 index.html 的那三個屬性(它由
   * nginx 分開服務),所以顏色只能自己問,而唯一趕得上首次繪製的時機就是這裡
   * —— router 的 `beforeLoad` 要等掛載之後才跑,已經晚了一幀。
   *
   * 失敗不擋掛載:`applySiteAppearance` 自己吞掉錯誤,這裡永遠會走到 render。
   */
  const mount = async () => {
    await applySiteAppearance();
    root.render(
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  };

  mount();
}
