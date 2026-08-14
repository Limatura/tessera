import createClient from "openapi-fetch";

import type { paths } from "./schema";

// Paths in the generated schema already include the server's mount prefix
// (see API_PREFIX in apps/server/src/api/mod.rs), so the base URL is root.
export const api = createClient<paths>({
  baseUrl: "/",
  credentials: "include",
});

export type { components, paths } from "./schema";
