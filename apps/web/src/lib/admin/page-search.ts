import { z } from "zod";

/** Rows per admin list page. The server caps it at 200 and returns 50 without `limit`. */
export const PAGE_SIZE = 20;

/**
 * The page lives in the search params instead of the path, so refreshing and
 * copying the URL return to the same page. A 1-based `page` instead of
 * `offset` keeps the URL readable.
 *
 * Not in the same file as `Pager`: `validateSearch` is not code-split, so a
 * schema in the component file would pull all of Pager into the entry bundle.
 */
export const pageSearchSchema = z.object({
  // oxlint-disable-next-line promise/prefer-await-to-then -- zod's catch, not a Promise's
  page: z.coerce.number().int().min(1).catch(1),
});
