import type { components } from "@tessera/api/schema";

type ErrorCode = components["schemas"]["ErrorCode"];

/**
 * The outcome of a bulk delete, shared by "my shares" and the admin panel:
 * what was deleted, and what was not and why. The server reports each one
 * (`DELETE /shares`, `DELETE /admin/shares`), so the dialog can keep only the
 * failures for a retry.
 */
export interface BulkDeleteResult {
  done: string[];
  failed: { item: string; error: { code: ErrorCode } | unknown }[];
}

/**
 * When the entire request is refused (signed out, no permission, over 200
 * items), every item becomes a failure with the same reason. The dialog then
 * needs no separate "whole batch failed" case: it shows the reason, and
 * delete retries them all.
 */
export function rejectAll(
  items: readonly string[],
  error: unknown
): BulkDeleteResult {
  return { done: [], failed: items.map((item) => ({ error, item })) };
}
