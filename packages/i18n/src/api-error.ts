import type { components } from "@tessera/api/schema";
import { formatBytes } from "@tessera/ui/lib/format";

export type ErrorCode = components["schemas"]["ErrorCode"];

type Message = () => string;
type MessageWith<Inputs> = (inputs: Inputs) => string;
interface Field {
  field: string;
}

/**
 * 翻譯錯誤需要的 Paraglide 訊息。字串本身在 `packages/i18n/messages`，兩個 app 的
 * `project.inlang` 都把那份併進自己的編譯結果，所以函式長在各自的 `@/paraglide`。
 *
 * 由 app 一個一個傳進來，而不是整個 `import * as m` 丟進來：namespace 一旦被當成
 * 值傳出去，bundler 就沒辦法再搖掉沒用到的訊息，整份語系檔會跟著進共用 chunk。
 */
export interface ApiErrorMessages {
  api_error_unknown: Message;

  api_error_not_found: Message;
  api_error_not_signed_in: Message;
  api_error_permission_denied: Message;
  api_error_session_required: Message;
  api_error_password_change_required: Message;
  api_error_payload_too_large: Message;
  api_error_payload_too_large_limit: MessageWith<{ limit: string }>;
  api_error_insufficient_storage_account: Message;
  api_error_insufficient_storage_role: Message;
  api_error_insufficient_storage_instance: Message;
  api_error_range_not_satisfiable: Message;
  api_error_too_many_requests: Message;
  api_error_too_many_requests_seconds: MessageWith<{ seconds: number }>;
  api_error_too_many_requests_minutes: MessageWith<{ minutes: number }>;
  api_error_scanning: Message;
  api_error_unsupported_media_type: Message;
  api_error_timeout: Message;
  api_error_unavailable: Message;
  api_error_internal: Message;
  api_error_bad_request: Message;

  api_error_field_unknown: Message;
  api_error_field_username: Message;
  api_error_field_email: Message;
  api_error_field_password: Message;
  api_error_field_code: Message;
  api_error_field_name: Message;
  api_error_field_url: Message;
  api_error_field_issuer: Message;
  api_error_field_title: Message;
  api_error_field_filename: Message;
  api_error_field_language: Message;
  api_error_field_max_uses: Message;
  api_error_field_expires_in_days: Message;

  api_error_value_empty: MessageWith<Field>;
  api_error_value_too_short: MessageWith<Field & { min: number }>;
  api_error_value_too_long: MessageWith<Field & { max: number }>;
  api_error_value_length: MessageWith<Field>;
  api_error_value_invalid: MessageWith<Field>;
  api_error_value_reserved: MessageWith<{ value: string }>;
  api_error_value_out_of_range: MessageWith<Field>;
  api_error_value_out_of_range_between: MessageWith<
    Field & { min: number; max: number }
  >;
  api_error_value_out_of_range_at_least: MessageWith<Field & { min: number }>;
  api_error_value_out_of_range_at_most: MessageWith<Field & { max: number }>;
  api_error_conflicting_fields: Message;

  api_error_rule_needs_digit: Message;
  api_error_rule_needs_mixed_case: Message;
  api_error_rule_needs_symbol: Message;
  api_error_rule_no_spaces: MessageWith<Field>;
  api_error_rule_missing_at: Message;
  api_error_rule_multiple_at: Message;
  api_error_rule_missing_domain: Message;
  api_error_rule_email: Message;
  api_error_rule_allowed_characters: Message;
  api_error_rule_starts_alphanumeric: Message;
  api_error_rule_code_alphabet: Message;
  api_error_rule_no_control_characters: MessageWith<Field>;
  api_error_rule_http_url: Message;

  api_error_invalid_credentials: Message;
  api_error_current_password_incorrect: Message;
  api_error_reauthentication_required: Message;
  api_error_password_breached: Message;
  api_error_registration_closed: Message;
  api_error_invite_required: Message;
  api_error_invite_unusable: Message;
  api_error_registration_link_unusable: Message;
  api_error_reset_link_unusable: Message;
  api_error_verification_link_unusable: Message;
  api_error_username_taken: Message;
  api_error_email_taken: Message;
  api_error_account_exists: Message;
  api_error_email_already_verified: Message;
  api_error_last_administrator: Message;
  api_error_last_sign_in_method: Message;

  api_error_unknown_permission: Message;
  api_error_role_name_taken: Message;
  api_error_builtin_role_immutable: Message;

  api_error_share_password_required: Message;
  api_error_share_password_incorrect: Message;
  api_error_share_expired: Message;
  api_error_share_exhausted: Message;
  api_error_share_content_removed: Message;
  api_error_share_is_file: Message;
  api_error_share_encrypted: Message;
  api_error_share_password_is_key: Message;
  api_error_share_is_text: Message;
  api_error_text_too_large: Message;
  api_error_text_too_large_limit: MessageWith<{ limit: string }>;
  api_error_code_space_exhausted: Message;
  api_error_archive_too_large_for_zip: Message;
  api_error_encryption_disabled: Message;
  api_error_encryption_invalid: Message;

  api_error_upload_interrupted: Message;
  api_error_upload_abandoned: Message;
  api_error_resumable_unsupported: Message;

  api_error_setting_unknown: MessageWith<{ key: string }>;
  api_error_setting_secret: MessageWith<{ key: string }>;
  api_error_setting_fixed_at_boot: MessageWith<{ key: string }>;
  api_error_setting_invalid: MessageWith<{ reason: string }>;
  api_error_confirmation_required: MessageWith<{ reason: string }>;
  api_error_provider_insecure: Message;
  api_error_provider_unreachable: MessageWith<{ reason: string }>;
  api_error_backend_unreachable: MessageWith<{ reason: string }>;
  api_error_backend_credential_unreadable: Message;
  api_error_backend_in_use: Message;
}

/** 從 `unknown` 讀出來的錯誤本體。每一項都可能沒有：斷線時拿到的是 `TypeError`。 */
interface ErrorLike {
  code?: unknown;
  message?: unknown;
  details?: unknown;
}

type Details = Record<string, unknown>;

const SECONDS_PER_MINUTE = 60;

function numberOf(details: Details, key: string): number | undefined {
  const value = details[key];
  return typeof value === "number" && Number.isFinite(value)
    ? value
    : undefined;
}

function stringOf(details: Details, key: string): string | undefined {
  const value = details[key];
  return typeof value === "string" && value !== "" ? value : undefined;
}

/**
 * 錯誤的 `code`。只認字串，不檢查是不是目前 `ErrorCode` 裡有的——server 比前端新的
 * 時候會送來前端還不認得的 code，呼叫端照樣能拿來比對。
 */
export function apiErrorCode(error: unknown): ErrorCode | undefined {
  if (typeof error !== "object" || error === null) {
    return undefined;
  }
  const { code } = error as ErrorLike;
  return typeof code === "string" ? (code as ErrorCode) : undefined;
}

/**
 * 依 `code` 把 server 的錯誤翻成目前語系的一句話，數字和欄位從 `details` 取。
 *
 * - 認得的 code：翻譯。
 * - 不認得的 code（server 比前端新）：退回 server 的英文 `message`，總比什麼都不說好。
 * - 根本沒有 code（斷線、前端自己丟的 `Error`）：退回 `fallback`。這時的 `message`
 *   是瀏覽器或程式內部的字串（`Failed to fetch`），不是寫給使用者看的。
 */
export function createApiErrorMessage(m: ApiErrorMessages) {
  const fieldLabel = (details: Details): string => {
    switch (stringOf(details, "field")) {
      case "username": {
        return m.api_error_field_username();
      }
      case "email": {
        return m.api_error_field_email();
      }
      case "password": {
        return m.api_error_field_password();
      }
      case "code": {
        return m.api_error_field_code();
      }
      case "name": {
        return m.api_error_field_name();
      }
      case "url": {
        return m.api_error_field_url();
      }
      case "issuer": {
        return m.api_error_field_issuer();
      }
      case "title": {
        return m.api_error_field_title();
      }
      case "filename": {
        return m.api_error_field_filename();
      }
      case "language": {
        return m.api_error_field_language();
      }
      // 建立邀請碼的 `uses` 跟列表上的 `max_uses` 是同一件事。
      case "uses":
      case "max_uses": {
        return m.api_error_field_max_uses();
      }
      case "expires_in_days": {
        return m.api_error_field_expires_in_days();
      }
      // 欄位名是 API 的 snake_case，直接放進句子裡對使用者沒有意義。
      default: {
        return m.api_error_field_unknown();
      }
    }
  };

  /**
   * 網域規則（帳號、密碼…）帶 `min`、`max`、`actual`，靠 `actual` 判斷是太短還太長；
   * 分享的標題、檔名只帶 `max`，那就只可能是太長。
   */
  const valueLength = (details: Details): string => {
    const field = fieldLabel(details);
    const min = numberOf(details, "min");
    const max = numberOf(details, "max");
    const actual = numberOf(details, "actual");
    const tooShort =
      min !== undefined &&
      (actual === undefined ? max === undefined : actual < min);
    const tooLong =
      max !== undefined &&
      (actual === undefined ? min === undefined : actual > max);
    if (tooShort) {
      return m.api_error_value_too_short({ field, min });
    }
    if (tooLong) {
      return m.api_error_value_too_long({ field, max });
    }
    return m.api_error_value_length({ field });
  };

  const valueInvalid = (details: Details): string => {
    const field = fieldLabel(details);
    switch (stringOf(details, "rule")) {
      case "needs_digit": {
        return m.api_error_rule_needs_digit();
      }
      case "needs_mixed_case": {
        return m.api_error_rule_needs_mixed_case();
      }
      case "needs_symbol": {
        return m.api_error_rule_needs_symbol();
      }
      case "no_spaces": {
        return m.api_error_rule_no_spaces({ field });
      }
      case "missing_at": {
        return m.api_error_rule_missing_at();
      }
      case "multiple_at": {
        return m.api_error_rule_multiple_at();
      }
      case "missing_domain": {
        return m.api_error_rule_missing_domain();
      }
      // 這幾條對使用者來說都是「這不是一個能用的地址」，分開講反而難懂。
      case "empty_part":
      case "local_part_too_long":
      case "empty_domain_label": {
        return m.api_error_rule_email();
      }
      case "allowed_characters": {
        return m.api_error_rule_allowed_characters();
      }
      case "starts_alphanumeric": {
        return m.api_error_rule_starts_alphanumeric();
      }
      case "code_alphabet": {
        return m.api_error_rule_code_alphabet();
      }
      case "no_control_characters": {
        return m.api_error_rule_no_control_characters({ field });
      }
      case "http_url": {
        return m.api_error_rule_http_url();
      }
      default: {
        return m.api_error_value_invalid({ field });
      }
    }
  };

  const valueOutOfRange = (details: Details): string => {
    const field = fieldLabel(details);
    const min = numberOf(details, "min");
    const max = numberOf(details, "max");
    if (min !== undefined && max !== undefined) {
      return m.api_error_value_out_of_range_between({ field, max, min });
    }
    if (min !== undefined) {
      return m.api_error_value_out_of_range_at_least({ field, min });
    }
    if (max !== undefined) {
      return m.api_error_value_out_of_range_at_most({ field, max });
    }
    return m.api_error_value_out_of_range({ field });
  };

  const tooManyRequests = (details: Details): string => {
    const seconds = numberOf(details, "retry_after_secs");
    if (seconds === undefined) {
      return m.api_error_too_many_requests();
    }
    if (seconds < SECONDS_PER_MINUTE) {
      return m.api_error_too_many_requests_seconds({
        seconds: Math.max(1, Math.ceil(seconds)),
      });
    }
    return m.api_error_too_many_requests_minutes({
      minutes: Math.ceil(seconds / SECONDS_PER_MINUTE),
    });
  };

  /**
   * 每個 code 怎麼翻。`Record<ErrorCode, …>` 讓 server 加了新 code、`schema.d.ts`
   * 重新產生之後這裡型別錯誤，提醒補上翻譯。
   *
   * 回 `null` 代表這次沒有翻譯可用，交給呼叫端退回 `message`。
   */
  const translations: Record<
    ErrorCode,
    (details: Details, message: string | undefined) => string | null
  > = {
    not_found: () => m.api_error_not_found(),
    not_signed_in: () => m.api_error_not_signed_in(),
    permission_denied: () => m.api_error_permission_denied(),
    session_required: () => m.api_error_session_required(),
    password_change_required: () => m.api_error_password_change_required(),
    payload_too_large: (details) => {
      const limit = numberOf(details, "limit_bytes");
      return limit === undefined
        ? m.api_error_payload_too_large()
        : m.api_error_payload_too_large_limit({ limit: formatBytes(limit) });
    },
    insufficient_storage: (details) => {
      switch (stringOf(details, "tier")) {
        case "account": {
          return m.api_error_insufficient_storage_account();
        }
        case "role": {
          return m.api_error_insufficient_storage_role();
        }
        default: {
          return m.api_error_insufficient_storage_instance();
        }
      }
    },
    range_not_satisfiable: () => m.api_error_range_not_satisfiable(),
    too_many_requests: tooManyRequests,
    scanning: () => m.api_error_scanning(),
    unsupported_media_type: () => m.api_error_unsupported_media_type(),
    timeout: () => m.api_error_timeout(),
    unavailable: () => m.api_error_unavailable(),
    internal: () => m.api_error_internal(),

    // 請求本身的形狀不對：使用者做不了什麼，只能重新整理再試。
    invalid_body: () => m.api_error_bad_request(),
    invalid_query: () => m.api_error_bad_request(),
    invalid_path: () => m.api_error_bad_request(),
    body_empty: () => m.api_error_bad_request(),
    body_incomplete: () => m.api_error_bad_request(),
    body_length_mismatch: () => m.api_error_bad_request(),
    invalid_hash_prefix: () => m.api_error_bad_request(),

    value_empty: (details) =>
      m.api_error_value_empty({ field: fieldLabel(details) }),
    value_length: valueLength,
    value_invalid: valueInvalid,
    value_reserved: (details) => {
      const value = stringOf(details, "value");
      return value === undefined
        ? m.api_error_value_invalid({ field: fieldLabel(details) })
        : m.api_error_value_reserved({ value });
    },
    value_out_of_range: valueOutOfRange,
    conflicting_fields: () => m.api_error_conflicting_fields(),

    invalid_credentials: () => m.api_error_invalid_credentials(),
    current_password_incorrect: () => m.api_error_current_password_incorrect(),
    reauthentication_required: () => m.api_error_reauthentication_required(),
    password_breached: () => m.api_error_password_breached(),
    registration_closed: () => m.api_error_registration_closed(),
    invite_required: () => m.api_error_invite_required(),
    invite_unusable: () => m.api_error_invite_unusable(),
    registration_link_unusable: () => m.api_error_registration_link_unusable(),
    reset_link_unusable: () => m.api_error_reset_link_unusable(),
    verification_link_unusable: () => m.api_error_verification_link_unusable(),
    username_taken: () => m.api_error_username_taken(),
    email_taken: () => m.api_error_email_taken(),
    account_exists: () => m.api_error_account_exists(),
    email_already_verified: () => m.api_error_email_already_verified(),
    last_administrator: () => m.api_error_last_administrator(),
    last_sign_in_method: () => m.api_error_last_sign_in_method(),

    unknown_permission: () => m.api_error_unknown_permission(),
    role_name_taken: () => m.api_error_role_name_taken(),
    builtin_role_immutable: () => m.api_error_builtin_role_immutable(),

    share_password_required: () => m.api_error_share_password_required(),
    share_password_incorrect: () => m.api_error_share_password_incorrect(),
    share_expired: () => m.api_error_share_expired(),
    share_exhausted: () => m.api_error_share_exhausted(),
    share_content_removed: () => m.api_error_share_content_removed(),
    share_is_file: () => m.api_error_share_is_file(),
    share_encrypted: () => m.api_error_share_encrypted(),
    share_password_is_key: () => m.api_error_share_password_is_key(),
    share_is_text: () => m.api_error_share_is_text(),
    text_too_large: (details) => {
      const limit = numberOf(details, "max_bytes");
      return limit === undefined
        ? m.api_error_text_too_large()
        : m.api_error_text_too_large_limit({ limit: formatBytes(limit) });
    },
    code_space_exhausted: () => m.api_error_code_space_exhausted(),
    archive_too_large_for_zip: () => m.api_error_archive_too_large_for_zip(),
    encryption_disabled: () => m.api_error_encryption_disabled(),
    encryption_invalid: () => m.api_error_encryption_invalid(),

    // 續傳協定內部的狀態對不上。使用者能做的只有重傳，細節留給 log。
    upload_offset_mismatch: () => m.api_error_upload_interrupted(),
    upload_exceeds_length: () => m.api_error_upload_interrupted(),
    upload_conflict: () => m.api_error_upload_interrupted(),
    upload_chunk_too_small: () => m.api_error_upload_interrupted(),
    upload_parts_mismatch: () => m.api_error_upload_interrupted(),
    upload_not_direct: () => m.api_error_upload_interrupted(),
    upload_direct_unavailable: () => m.api_error_upload_interrupted(),
    upload_abandoned: () => m.api_error_upload_abandoned(),
    resumable_unsupported: () => m.api_error_resumable_unsupported(),

    setting_unknown: (details) => {
      const key = stringOf(details, "key");
      return key === undefined ? null : m.api_error_setting_unknown({ key });
    },
    setting_secret: (details) => {
      const key = stringOf(details, "key");
      return key === undefined ? null : m.api_error_setting_secret({ key });
    },
    setting_fixed_at_boot: (details) => {
      const key = stringOf(details, "key");
      return key === undefined
        ? null
        : m.api_error_setting_fixed_at_boot({ key });
    },
    /*
     * 下面這幾個真正的原因（哪個值、哪裡連不上、會刪掉多少）只寫在 server 的
     * `message` 裡，details 沒有。翻譯前半句、把原因原樣接在後面，給管理員看的
     * 診斷資訊不能丟。
     */
    setting_invalid: (_, message) =>
      message === undefined
        ? null
        : m.api_error_setting_invalid({ reason: message }),
    confirmation_required: (_, message) =>
      message === undefined
        ? null
        : m.api_error_confirmation_required({ reason: message }),
    provider_insecure: () => m.api_error_provider_insecure(),
    provider_unreachable: (_, message) =>
      message === undefined
        ? null
        : m.api_error_provider_unreachable({ reason: message }),
    backend_unreachable: (_, message) =>
      message === undefined
        ? null
        : m.api_error_backend_unreachable({ reason: message }),
    backend_credential_unreadable: () =>
      m.api_error_backend_credential_unreadable(),
    backend_in_use: () => m.api_error_backend_in_use(),
    // 啟動設定錯誤，server 說不會送到 client。
    invalid_configuration: () => null,
  };

  return function apiErrorMessage(error: unknown, fallback?: string): string {
    const fallbackMessage = fallback ?? m.api_error_unknown();
    const code = apiErrorCode(error);
    if (code === undefined) {
      return fallbackMessage;
    }
    const body = error as ErrorLike;
    const message =
      typeof body.message === "string" && body.message !== ""
        ? body.message
        : undefined;
    const details =
      typeof body.details === "object" && body.details !== null
        ? (body.details as Details)
        : {};
    // `Object.hasOwn`：前端還不認得的 code 不在表裡，而且不能誤撞到 `toString` 之類。
    const translate = Object.hasOwn(translations, code)
      ? translations[code]
      : undefined;
    return translate?.(details, message) ?? message ?? fallbackMessage;
  };
}
