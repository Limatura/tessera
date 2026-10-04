import {
  MutationCache,
  QueryCache,
  QueryClient,
  QueryClientProvider,
} from "@tanstack/react-query";
import { RouterProvider, createRouter } from "@tanstack/react-router";
import { Loader } from "@tessera/ui/components/loader";
import ReactDOM from "react-dom/client";

import { sessionQueryOptions } from "@/hooks/use-auth";
import { apiErrorCode } from "@/lib/api-error";
import { CHANGE_PASSWORD_PATH } from "@/lib/route-guard";

import { routeTree } from "./routeTree.gen";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
    },
  },
  mutationCache: new MutationCache({ onError: redirectIfPasswordExpired }),
  queryCache: new QueryCache({ onError: redirectIfPasswordExpired }),
});

const router = createRouter({
  context: { queryClient },
  defaultPendingComponent: () => <Loader />,
  defaultPreload: "intent",
  routeTree,
  scrollRestoration: true,
});

/**
 * 任何請求回 `password_change_required` 就送去改密碼頁。
 *
 * 進站時 root 的 guard 已經看過 session，這裡接的是之後才變的情況（例如管理員在
 * 你登入期間重設了密碼）。順手把 session 快取標成要改密碼，之後換頁 guard 才擋得住。
 */
function redirectIfPasswordExpired(error: unknown) {
  if (apiErrorCode(error) !== "password_change_required") {
    return;
  }
  queryClient.setQueryData(
    sessionQueryOptions.queryKey,
    (account) => account && { ...account, must_change_password: true }
  );
  const { href, pathname } = router.state.location;
  if (!pathname.startsWith(CHANGE_PASSWORD_PATH)) {
    router.navigate({ search: { redirect: href }, to: CHANGE_PASSWORD_PATH });
  }
}

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
  root.render(
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  );
}
