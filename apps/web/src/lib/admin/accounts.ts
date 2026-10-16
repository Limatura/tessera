import type { components } from "@tessera/api/schema";

export type AdminUser = components["schemas"]["AdminUserResponse"];

/**
 * A deleted account does not disappear; it becomes a `gone-<id>` /
 * `<id>@deleted.invalid` leftover with `state` set to `deleted`.
 */
export function isDeletedAccount(account: AdminUser): boolean {
  return account.state === "deleted";
}

/**
 * The server lets no one register this prefix (see `TOMBSTONE_PREFIX` in
 * crates/tessera-core/src/username.rs), so the prefix alone identifies one.
 */
const TOMBSTONE_PREFIX = "gone-";

/**
 * For places that have only the username string and no `state` (such as
 * `owner_username`). With the full account object, use `isDeletedAccount`.
 */
export function isDeletedUsername(username: string): boolean {
  return username.startsWith(TOMBSTONE_PREFIX);
}
