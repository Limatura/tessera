import {
  Outlet,
  createFileRoute,
  useRouterState,
} from "@tanstack/react-router";

import { Header } from "@/components/header";
import { PageTransition } from "@/components/page-transition";

/**
 * The shared frame of the public pages: the Header on top and a fade between pages.
 *
 * Pathless (`_site`), so it takes no URL segment. The admin panel at `/admin` has its
 * own sidebar frame without this Header, so the Header cannot live in root.
 *
 * The animation key is the pathname without search: switching the home page's upload
 * tab only changes the query and must not replay it. The wrapper is the second row of
 * the `__root` grid and needs `min-w-0`, for the same reason as `__root`'s grid-cols-1.
 */
export const Route = createFileRoute("/_site")({
  component: SiteLayout,
});

function SiteLayout() {
  const pathname = useRouterState({
    select: (state) => state.location.pathname,
  });

  return (
    <>
      <Header />
      <PageTransition className="min-w-0" transitionKey={pathname}>
        <Outlet />
      </PageTransition>
    </>
  );
}
