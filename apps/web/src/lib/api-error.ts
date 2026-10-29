import type { components } from "@tessera/api/schema";
import { MalformedImageError } from "@tessera/metadata";
import { formatBytes } from "@tessera/ui/lib/format";

import type { BulkDeleteResult } from "@/lib/bulk-delete";
import * as m from "@/paraglide/messages";

type ErrorCode = components["schemas"]["ErrorCode"];

/**
 * The error body read from `unknown`. Any field may be missing: a dropped
 * connection gives a `TypeError`.
 */
interface ErrorLike {
  code?: unknown;
  message?: unknown;
  details?: unknown;
  request_id?: unknown;
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
 * The error's `code`. Any string is accepted, not only current `ErrorCode`s: a
 * server newer than the frontend sends codes it does not know yet, and callers
 * can still compare them.
 */
export function apiErrorCode(error: unknown): ErrorCode | undefined {
  if (typeof error !== "object" || error === null) {
    return undefined;
  }
  const { code } = error as ErrorLike;
  return typeof code === "string" ? (code as ErrorCode) : undefined;
}

function fieldLabel(details: Details): string {
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
    // `uses` when creating an invitation is the list's `max_uses`.
    case "uses":
    case "max_uses": {
      return m.api_error_field_max_uses();
    }
    case "expires_in_days": {
      return m.api_error_field_expires_in_days();
    }
    case "events": {
      return m.api_error_field_events();
    }
    case "webhooks": {
      return m.api_error_field_webhooks();
    }
    // Field names are the API's snake_case and mean nothing to a user inside a sentence.
    default: {
      return m.api_error_field_unknown();
    }
  }
}

/**
 * Domain rules (username, password, ...) send `min`, `max` and `actual`, and
 * `actual` tells too short from too long. A share's title or file name sends
 * only `max`, so it can only be too long.
 */
function valueLength(details: Details): string {
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
}

function valueInvalid(details: Details): string {
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
    // To a user all of these mean "not a usable address"; separate messages would only confuse.
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
    case "http_url":
    case "not_a_url": {
      return m.api_error_rule_http_url();
    }
    case "not_public": {
      return m.api_error_rule_not_public();
    }
    default: {
      return m.api_error_value_invalid({ field });
    }
  }
}

function valueOutOfRange(details: Details): string {
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
}

function tooManyRequests(details: Details): string {
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
}

/**
 * How each code is translated. `Record<ErrorCode, …>` turns a code the server
 * adds into a type error once `schema.d.ts` is regenerated, as a reminder to
 * translate it.
 *
 * `null` means no translation this time; the caller falls back to `message`.
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

  // The request itself is malformed. Nothing the user can do but reload and retry.
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
  mfa_code_incorrect: () => m.api_error_mfa_code_incorrect(),
  mfa_challenge_unusable: () => m.api_error_mfa_challenge_unusable(),
  mfa_already_enabled: () => m.api_error_mfa_already_enabled(),
  passkey_rejected: () => m.api_error_passkey_rejected(),
  device_authorization_pending: () =>
    m.api_error_device_authorization_pending(),
  device_authorization_denied: () => m.api_error_device_authorization_denied(),
  device_code_unusable: () => m.api_error_device_code_unusable(),
  passkey_invalid: () => m.api_error_passkey_invalid(),
  passkey_already_registered: () => m.api_error_passkey_already_registered(),
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

  // Resumable upload state out of sync. The user can only re-upload; details go to the log.
  upload_offset_mismatch: () => m.api_error_upload_interrupted(),
  upload_exceeds_length: () => m.api_error_upload_interrupted(),
  upload_conflict: () => m.api_error_upload_interrupted(),
  upload_chunk_too_small: () => m.api_error_upload_interrupted(),
  upload_parts_mismatch: () => m.api_error_upload_interrupted(),
  upload_not_direct: () => m.api_error_upload_interrupted(),
  upload_direct_unavailable: () => m.api_error_upload_interrupted(),
  upload_abandoned: () => m.api_error_upload_abandoned(),
  resumable_unsupported: () => m.api_error_resumable_unsupported(),
  anonymous_upload_disabled: () => m.api_error_anonymous_upload_disabled(),
  file_type_blocked: (details) => {
    const extension = stringOf(details, "extension");
    return extension === undefined
      ? m.api_error_file_type_blocked()
      : m.api_error_file_type_blocked_extension({ extension: `.${extension}` });
  },

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
   * For these, the real cause (which value, what is unreachable, how much
   * would be deleted) is only in the server's `message`, not in details.
   * Translate the first half and append the cause verbatim: an administrator
   * needs that diagnostic.
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
  backend_name_taken: () => m.api_error_backend_name_taken(),
  backend_credentials_required: () =>
    m.api_error_backend_credentials_required(),
  // Startup configuration error; the server says it is never sent to clients.
  invalid_configuration: () => null,
};

const HTTP_PAYLOAD_TOO_LARGE = 413;
const HTTP_BAD_GATEWAY = 502;
const HTTP_GATEWAY_TIMEOUT = 504;

/**
 * An error with no `code`, so the server did not write it. Two kinds still say
 * something useful: an upload the uploader marked `network` (no answer at all:
 * offline, or a file the browser could not read), and a refusal from something
 * in front of the server, such as a proxy's 413 or 502, whose body is not ours.
 * A bare `TypeError` from `fetch` stays with the caller's fallback, which knows
 * what was being attempted.
 */
function codelessErrorMessage(error: unknown): string | undefined {
  if (error instanceof MalformedImageError) {
    return m.api_error_image_malformed({ name: error.fileName });
  }
  if (typeof error !== "object" || error === null) {
    return undefined;
  }
  const { network, status } = error as { network?: unknown; status?: unknown };
  if (network === true) {
    return m.api_error_network();
  }
  if (status === HTTP_PAYLOAD_TOO_LARGE) {
    return m.api_error_payload_too_large();
  }
  if (
    typeof status === "number" &&
    status >= HTTP_BAD_GATEWAY &&
    status <= HTTP_GATEWAY_TIMEOUT
  ) {
    return m.api_error_unavailable();
  }
  return undefined;
}

/**
 * Translate a server error into one sentence in the current locale, with
 * numbers and field names taken from `details`.
 *
 * - Known code: translated.
 * - Unknown code (server newer than the frontend): the server's English
 *   `message`, better than saying nothing.
 * - No code at all (dropped connection, an `Error` thrown by the frontend):
 *   `fallback`. The `message` then is a browser or internal string
 *   (`Failed to fetch`), not written for users.
 *
 * A server error carries `request_id`, the id its log line has too; the
 * sentence ends with it, so a person reporting the error can quote it.
 */
export function apiErrorMessage(error: unknown, fallback?: string): string {
  const fallbackMessage = fallback ?? m.api_error_unknown();
  const code = apiErrorCode(error);
  if (code === undefined) {
    return codelessErrorMessage(error) ?? fallbackMessage;
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
  // `Object.hasOwn`: codes the frontend does not know are absent, and must
  // not hit `toString` and the like.
  const translate = Object.hasOwn(translations, code)
    ? translations[code]
    : undefined;
  const sentence = translate?.(details, message) ?? message ?? fallbackMessage;
  const requestId =
    typeof body.request_id === "string" && body.request_id !== ""
      ? body.request_id
      : undefined;
  return requestId === undefined
    ? sentence
    : m.api_error_with_request_id({ message: sentence, requestId });
}

/**
 * The sentence for a bulk delete that left some shares behind. With one
 * failure it gives the server's reason; with several, each with its own
 * reason, it gives only the count.
 */
export function settledErrorMessage(
  failed: BulkDeleteResult["failed"],
  fallback: string
): string {
  const [first] = failed;
  if (failed.length === 1 && first) {
    return apiErrorMessage(
      first.error as Partial<components["schemas"]["ErrorBody"]>,
      fallback
    );
  }
  return m.shares_bulk_delete_failed({ count: failed.length });
}
