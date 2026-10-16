import { createFileRoute, redirect } from "@tanstack/react-router";

/**
 * `/settings` has no page of its own. It sends old links, bookmarks and the
 * sidebar's "Settings" entry to the first tab. Permissions were checked in
 * `route.tsx` one level up.
 */
export const Route = createFileRoute("/admin/settings/")({
  beforeLoad: () => {
    throw redirect({ to: "/admin/settings/site" });
  },
});
