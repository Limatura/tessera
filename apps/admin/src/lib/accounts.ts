import type { Account } from "./permissions";

/**
 * 刪掉的帳號不會消失,會變成 `gone-<id>` / `<id>@deleted.invalid` 的殘骸。
 * server 不讓任何人註冊這個前綴(見 crates/tessera-core/src/username.rs 的
 * `TOMBSTONE_PREFIX`),所以看名稱就分得出來。
 *
 * `AccountResponse` 目前沒有 `state` 欄位,之後有了應改看那個。
 */
const TOMBSTONE_PREFIX = "gone-";

export function isDeletedAccount(account: Account): boolean {
  return account.username.startsWith(TOMBSTONE_PREFIX);
}
