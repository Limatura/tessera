import {
  MutationCache,
  QueryCache,
  QueryClient,
  QueryClientProvider,
} from "@tanstack/react-query";
import { RouterProvider, createRouter } from "@tanstack/react-router";
import { sessionQueryOptions } from "@tessera/auth/hooks";
import { Loader } from "@tessera/ui/components/loader";
import ReactDOM from "react-dom/client";

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

// Admin data changes often, so refetching on tab focus (the default) beats a
// stale list; no refetch within 30 seconds, so moving between admin pages does
// not flash a loading state each time.
queryClient.setQueryDefaults(["admin"], { staleTime: 30_000 });

const router = createRouter({
  context: { queryClient },
  defaultPendingComponent: () => <Loader />,
  defaultPreload: "intent",
  routeTree,
  scrollRestoration: true,
});

/**
 * Sends any request answered with `password_change_required` to the change
 * password page.
 *
 * The root guard already checked the session on entry; this catches changes
 * after that (an administrator resetting the password while you are signed
 * in, say). It also marks the cached session as needing a change, so the
 * navigation guard can hold.
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

  interface StaticDataRouteOption {
    /**
     * Text for the admin header, see `routes/admin/route.tsx`. A message function, so it
     * follows a language switch.
     */
    title?: () => string;
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
