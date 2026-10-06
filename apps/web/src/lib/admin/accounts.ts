import type { components } from "@tessera/api/schema";

export type AdminUser = components["schemas"]["AdminUserResponse"];

/**
 * 刪掉的帳號不會消失,會變成 `gone-<id>` / `<id>@deleted.invalid` 的殘骸,
 * `state` 是 `deleted`。
 */
export function isDeletedAccount(account: AdminUser): boolean {
  return account.state === "deleted";
}

/**
 * server 不讓任何人註冊這個前綴(見 crates/tessera-core/src/username.rs 的
 * `TOMBSTONE_PREFIX`),所以只有名稱時看前綴也分得出來。
 */
const TOMBSTONE_PREFIX = "gone-";

/**
 * 只有 username 字串、拿不到 `state` 的地方(例如 `owner_username`)用這個。
 * 有整個帳號物件時用 `isDeletedAccount`。
 */
export function isDeletedUsername(username: string): boolean {
  return username.startsWith(TOMBSTONE_PREFIX);
}
