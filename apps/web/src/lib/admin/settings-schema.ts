import {
  APPEARANCE_ACCENTS,
  APPEARANCE_MODES,
  APPEARANCE_THEMES,
} from "@tessera/ui/lib/appearance";
import { z } from "zod";

import {
  PASSWORD_MAX_CEILING,
  PASSWORD_MIN_FLOOR,
} from "@/lib/api/password-policy";
import * as m from "@/paraglide/messages";

/**
 * Form fields map to the dotted keys listed in `apps/server/SETTINGS.md`, with
 * names converted to camelCase.
 *
 * Three kinds of key are not taken here:
 * - `FIXED_AT_BOOT` (such as `server.max_body_bytes`): a PUT rejects the whole
 * batch, so they are shown read-only.
 * - `SECRET` (such as `database.url`): neither readable nor writable
 * here, only through a dedicated endpoint.
 * - `SEALED` (such as `*.client_secret`, `mail.password`,
 * `scan.authorization`): writable but never read back; empty means unchanged.
 *
 * Error messages are always `error: () => m.xxx()`. The schema is a constant
 * built once at module load, so calling `m.xxx()` directly would bake in the
 * language at that moment; the function wrapper follows language switches.
 */

// Appearance: appearance.*
// The three enums live in `@tessera/ui/lib/appearance` and must match
// `appearance.rs`; this only re-exports them.
export {
  APPEARANCE_ACCENTS,
  APPEARANCE_MODES,
  APPEARANCE_THEMES,
} from "@tessera/ui/lib/appearance";

export const appearanceSettingsSchema = z.object({
  // appearance.accent
  accent: z.enum(APPEARANCE_ACCENTS),
  // appearance.mode
  mode: z.enum(APPEARANCE_MODES),
  // appearance.theme
  theme: z.enum(APPEARANCE_THEMES),
});

export type AppearanceSettingsValues = z.infer<typeof appearanceSettingsSchema>;

// Site information: site.*

/** Upper bound of `site.title` (`SiteConfig::MAX_TITLE`). */
export const SITE_TITLE_MAX = 100;

/** Upper bound of `site.description` (`SiteConfig::MAX_DESCRIPTION`). */
export const SITE_DESCRIPTION_MAX = 300;

/**
 * The server counts length with `chars().count()`, in Unicode code points.
 * `.length` counts UTF-16 code units, where an emoji counts as two, so the
 * frontend would reject before the server does.
 */
function codePoints(value: string): number {
  return [...value].length;
}

/**
 * `site.language`: the site's default language. Share link previews (meta
 * tags, the `og.png` card) are written in it, since crawlers send no language
 * preference, and the web UI opens in it for a visitor with no saved choice
 * whose browser asks for none of the languages the app ships.
 */
export const SITE_LANGUAGES = ["en", "zh-TW", "zh-CN", "ja"] as const;

export const siteSettingsSchema = z.object({
  // site.description: empty means this meta tag is not emitted
  description: z
    .string()
    .refine((value) => codePoints(value) <= SITE_DESCRIPTION_MAX, {
      error: () => m.settings_schema_max_chars({ max: SITE_DESCRIPTION_MAX }),
    }),
  // site.language
  language: z.enum(SITE_LANGUAGES),
  // site.title
  title: z
    .string()
    .refine((value) => value.trim() !== "", {
      error: () => m.settings_schema_site_title_required(),
    })
    .refine((value) => codePoints(value) <= SITE_TITLE_MAX, {
      error: () => m.settings_schema_max_chars({ max: SITE_TITLE_MAX }),
    }),
});

export type SiteSettingsValues = z.infer<typeof siteSettingsSchema>;

// Storage: server.part_size_bytes + storage.split_across_backends
//      + storage.reconcile_interval_hours / reconcile_grace_hours
//
// Storage locations are not settings: each backend is a row in the
// `storage_backends` table, managed through `/admin/storage/backends`, not
// through `PUT /admin/settings` here.

/** S3 rejects parts smaller than 5 MiB, except the last. */
export const MIN_PART_SIZE_BYTES = 5 * 1024 * 1024;

export const storageSettingsSchema = z.object({
  // server.part_size_bytes
  partSizeBytes: z
    .number()
    .int()
    .min(MIN_PART_SIZE_BYTES, {
      error: () => m.settings_schema_min_part_size(),
    }),
  // storage.reconcile_grace_hours
  reconcileGraceHours: z.number().int().min(0),
  // storage.reconcile_interval_hours: 0 disables reconciliation
  reconcileIntervalHours: z.number().int().min(0),
  // storage.split_across_backends
  splitAcrossBackends: z.boolean(),
});

export type StorageSettingsValues = z.infer<typeof storageSettingsSchema>;

// Uploads and retention: features.anonymous_upload + storage.max_total_bytes
//      + share.default_expiry_hours / max_expiry_hours
//
// `server.max_body_bytes` is not here: it is FIXED_AT_BOOT and shown read-only.
// `auth.throttle.source.*` used to be on this page, but it covers every
// expensive operation from one source, not only uploads, so it moved to the
// rate limit page.
const BYTES_MIN = 0;

export const uploadSettingsSchema = z
  // oxlint-disable-next-line sort-keys -- grouped by who can upload / retention / capacity; alphabetical order would scatter the meaning
  .object({
    // features.anonymous_upload
    allowAnonymousUpload: z.boolean(),
    // share.default_expiry_hours; 0 means use maxExpiryHours
    defaultExpiryHours: z.number().int().min(0),
    // share.max_expiry_hours; 0 means no limit
    maxExpiryHours: z.number().int().min(0),
    // storage.max_total_bytes
    maxTotalBytes: z.number().int().min(BYTES_MIN),
  })
  .refine(
    (values) =>
      values.maxExpiryHours === 0 ||
      values.defaultExpiryHours === 0 ||
      values.defaultExpiryHours <= values.maxExpiryHours,
    {
      error: () => m.settings_schema_expiry_order(),
      path: ["defaultExpiryHours"],
    }
  );

export type UploadSettingsValues = z.infer<typeof uploadSettingsSchema>;

/** `MAX_LOGIN_HISTORY_DAYS` in config.rs (two years). */
export const MAX_LOGIN_HISTORY_DAYS = 365 * 2;

// Accounts and registration: auth.registration / auth.default_role
//      / auth.session_ttl_hours / auth.login_history_days

/** The server's `auth.registration`; values are sent as is. */
export const REGISTRATION_MODES = ["open", "invite", "closed"] as const;

export const accountSettingsSchema = z.object({
  // auth.default_role: a role name, not an id; options come from GET /admin/roles
  defaultRole: z
    .string()
    .min(1, { error: () => m.settings_schema_select_role() }),
  // auth.login_history_days
  loginHistoryDays: z
    .number()
    .int()
    .min(1, { error: () => m.settings_schema_at_least_days({ n: 1 }) })
    .max(MAX_LOGIN_HISTORY_DAYS, {
      error: () => m.settings_schema_max_value({ max: MAX_LOGIN_HISTORY_DAYS }),
    }),
  // auth.registration
  registration: z.enum(REGISTRATION_MODES),
  // auth.session_ttl_hours
  sessionTtlHours: z
    .number()
    .int()
    .min(1, { error: () => m.settings_schema_at_least_hours({ n: 1 }) }),
});

export type AccountSettingsValues = z.infer<typeof accountSettingsSchema>;

// Sign-in methods: auth.oidc.* / auth.github.* / auth.discord.*

/** The server's `ProviderRegistration`: whether each provider may create new accounts. */
export const PROVIDER_REGISTRATION_MODES = [
  "inherit",
  "always",
  "never",
] as const;

export type ProviderRegistrationMode =
  (typeof PROVIDER_REGISTRATION_MODES)[number];

/** The server's `LinkByEmail`. Only OIDC has three modes; GitHub / Discord are still a toggle. */
export const LINK_BY_EMAIL_MODES = [
  "never",
  "when_verified",
  "always",
] as const;

const HTTPS_URL = /^https:\/\/[^\s/][^\s]*$/u;
/** `is_private_or_tls` in `auth/oidc/mod.rs`: plain http only on these hosts. */
const LOOPBACK_HTTP_URL =
  /^http:\/\/(?:127\.0\.0\.1|localhost|\[::1\])(?:[:/?#]|$)/u;
const ABSOLUTE_URL = /^https?:\/\/[^\s/][^\s]*$/u;

/**
 * OIDC. An empty `issuer` turns the feature off, so there is no separate
 * enable switch: another boolean would only make "issuer filled in but not
 * turned on" a possible state.
 */
export const oidcSettingsSchema = z
  .object({
    // auth.oidc.allow_registration
    allowRegistration: z.enum(PROVIDER_REGISTRATION_MODES),
    // auth.oidc.client_id
    clientId: z.string(),
    // auth.oidc.client_secret (SEALED, empty means unchanged)
    clientSecret: z.string(),
    // auth.oidc.display_name
    displayName: z.string(),
    // auth.oidc.group_claim
    groupClaim: z.string(),
    // auth.oidc.issuer: the spec requires an exact string match with the `iss` claim, and https
    issuer: z
      .string()
      .refine(
        (value) =>
          value === "" ||
          HTTPS_URL.test(value) ||
          LOOPBACK_HTTP_URL.test(value),
        {
          error: () => m.settings_schema_oidc_issuer_https(),
        }
      )
      // Pasting the discovery URL is a common mistake that passes the check
      // above, but the server appends `/.well-known/...` itself, so every sign-in
      // would fail
      .refine((value) => !value.includes("/.well-known/"), {
        error: () => m.settings_schema_oidc_issuer_not_discovery_url(),
      }),
    // auth.oidc.link_by_email
    linkByEmail: z.enum(LINK_BY_EMAIL_MODES),
    // auth.oidc.required_group: empty means anyone the provider authenticates may sign in
    requiredGroup: z.string(),
    // auth.oidc.scopes: string[] on the server, a space-separated string in the form
    scopes: z.string(),
  })
  // `OidcConfig::validate`: once an issuer is set the claim cannot be blank
  .refine(
    (values) => values.issuer.trim() === "" || values.groupClaim.trim() !== "",
    {
      error: () => m.settings_schema_oidc_group_claim_required(),
      path: ["groupClaim"],
    }
  );

export type OidcSettingsValues = z.infer<typeof oidcSettingsSchema>;

/**
 * GitHub and Discord. Both have the same fields, except `baseUrl` is only for
 * GitHub (GitHub Enterprise Server); the Discord card neither draws that field
 * nor sends that key.
 */
export const oauthSettingsSchema = z.object({
  // auth.{github,discord}.allow_registration
  allowRegistration: z.enum(PROVIDER_REGISTRATION_MODES),
  // auth.{github,discord}.base_url
  baseUrl: z
    .string()
    .refine((value) => value === "" || ABSOLUTE_URL.test(value), {
      error: () => m.settings_schema_absolute_url(),
    }),
  // auth.{github,discord}.client_id
  clientId: z.string(),
  // auth.{github,discord}.client_secret (SEALED, empty means unchanged)
  clientSecret: z.string(),
  // auth.{github,discord}.display_name
  displayName: z.string(),
  // auth.{github,discord}.link_by_verified_email
  linkByVerifiedEmail: z.boolean(),
  // auth.{github,discord}.scopes
  scopes: z.string(),
});

export type OauthSettingsValues = z.infer<typeof oauthSettingsSchema>;

// Scanning: scan.*

/** The server takes only these: clamd's `INSTREAM`, or an HTTP service the file is POSTed to. */
const SCAN_URL_PATTERN = /^(?:clamd|https?):\/\/\S+$/u;
/** `ScanConfig::validate`: a clamd address needs a host and an explicit port. */
const CLAMD_URL_PATTERN = /^clamd:\/\/(?:\[[^\]\s]+\]|[^\s/:[\]]+):\d+\/?$/u;

export const scanSettingsSchema = z.object({
  // scan.authorization (SEALED, empty means unchanged)
  authorization: z.string(),
  // scan.url: empty means no scanning
  url: z
    .string()
    .trim()
    .refine(
      (value) =>
        value === "" ||
        (SCAN_URL_PATTERN.test(value) &&
          (!value.startsWith("clamd:") || CLAMD_URL_PATTERN.test(value))),
      {
        error: () => m.settings_schema_scan_url_invalid(),
      }
    ),
});

export type ScanSettingsValues = z.infer<typeof scanSettingsSchema>;

// Mail: mail.*

/**
 * Upper bound of `mail.reset_ttl_minutes` (24 hours), matching the constant of
 * the same name in config.rs.
 */
export const MAX_RESET_TTL_MINUTES = 24 * 60;

/** Upper bound of `mail.registration_ttl_hours` (7 days). */
export const MAX_REGISTRATION_TTL_HOURS = 7 * 24;

const MAX_PORT = 65_535;

/** `host` / `from` / `public_url` are all set or all empty; the server rejects a partial setup. */
const MAIL_TRIO = ["host", "from", "publicUrl"] as const;

/**
 * The server's `mail.tls`. `none` is only for a local or internal relay; see
 * the form's warning.
 */
export const MAIL_TLS_MODES = ["auto", "starttls", "implicit", "none"] as const;

export const mailSettingsSchema = z
  // oxlint-disable-next-line sort-keys -- grouped by connection / sender / lifetimes
  .object({
    // mail.host: empty turns off all mail
    host: z.string(),
    // mail.port
    port: z
      .number()
      .int()
      .min(1, { error: () => m.settings_schema_at_least({ n: 1 }) })
      .max(MAX_PORT),
    // mail.tls
    tls: z.enum(MAIL_TLS_MODES),
    // mail.username (SEALED, empty means unchanged)
    username: z.string(),
    // mail.password (SEALED, empty means unchanged)
    password: z.string(),
    // mail.from
    from: z.string(),
    // mail.public_url: base for links in mail, by design not derived from the request's Host header
    publicUrl: z.string(),
    // mail.reset_ttl_minutes
    resetTtlMinutes: z
      .number()
      .int()
      .min(1, { error: () => m.settings_schema_at_least_minutes({ n: 1 }) })
      .max(MAX_RESET_TTL_MINUTES, {
        error: () => m.settings_schema_reset_ttl_max(),
      }),
    // mail.registration_ttl_hours
    registrationTtlHours: z
      .number()
      .int()
      .min(1, { error: () => m.settings_schema_at_least_hours({ n: 1 }) })
      .max(MAX_REGISTRATION_TTL_HOURS, {
        error: () => m.settings_schema_registration_ttl_max(),
      }),
  })
  .superRefine((values, ctx) => {
    if (values.from.trim() !== "" && !values.from.includes("@")) {
      ctx.addIssue({
        code: "custom",
        message: m.settings_schema_mail_from_required(),
        path: ["from"],
      });
    }

    if (
      values.publicUrl.trim() !== "" &&
      !ABSOLUTE_URL.test(values.publicUrl)
    ) {
      ctx.addIssue({
        code: "custom",
        message: m.settings_schema_absolute_url(),
        path: ["publicUrl"],
      });
    }

    // A partial setup would make the server reject the batch, other valid
    // fields included, so stop it here and name the missing field
    const missing = MAIL_TRIO.filter((name) => values[name].trim() === "");
    if (missing.length === 0 || missing.length === MAIL_TRIO.length) {
      return;
    }

    for (const name of missing) {
      ctx.addIssue({
        code: "custom",
        message: m.settings_schema_mail_trio_incomplete(),
        path: [name],
      });
    }
  });

export type MailSettingsValues = z.infer<typeof mailSettingsSchema>;

// Network and reverse proxy: server.*

/** The server's `auth.cookie_secure`. */
export const COOKIE_SECURE_POLICIES = ["auto", "always", "never"] as const;

export const networkSettingsSchema = z
  // oxlint-disable-next-line sort-keys -- grouped by public address / proxies / timeouts
  .object({
    // server.public_url: must be absolute; the server strips a trailing slash
    publicUrl: z
      .string()
      .refine((value) => value === "" || ABSOLUTE_URL.test(value), {
        error: () => m.settings_schema_absolute_url(),
      }),
    // server.trusted_proxies: string[] on the server, one per line in the form
    trustedProxies: z.string(),
    // auth.cookie_secure
    cookieSecure: z.enum(COOKIE_SECURE_POLICIES),
    // server.request_timeout_secs: not applied to upload routes
    requestTimeoutSecs: z
      .number()
      .int()
      .min(1, { error: () => m.settings_schema_at_least_seconds({ n: 1 }) }),
    // server.upload_stall_secs
    uploadStallSecs: z
      .number()
      .int()
      .min(1, { error: () => m.settings_schema_at_least_seconds({ n: 1 }) }),
  });

export type NetworkSettingsValues = z.infer<typeof networkSettingsSchema>;

// Rate limits: auth.throttle.*

/**
 * Nine counters protecting different things, each `{max_attempts, window_secs}`.
 * Neither value may be 0: zero attempts locks everyone out (for `source`, the
 * whole machine), and a zero-second window never blocks anything. The order is
 * the display order: `source`, with the widest effect, first, the rest grouped
 * by what they protect.
 */
export const THROTTLE_COUNTERS = [
  "source",
  "login",
  "share",
  "lookup",
  "reset",
  "registration",
  "verification",
  "speed",
  "pwned",
] as const;

export type ThrottleCounter = (typeof THROTTLE_COUNTERS)[number];

/** Upper bound of `auth.throttle.max_tracked_keys`. A memory cap, not a security setting. */
export const MAX_TRACKED_KEYS_CEILING = 100_000;

function throttleAttempts() {
  return z
    .number()
    .int()
    .min(1, { error: () => m.settings_schema_throttle_attempts_floor() });
}

function throttleWindow() {
  return z
    .number()
    .int()
    .min(1, { error: () => m.settings_schema_throttle_window_floor() });
}

/**
 * Fields are flattened (`loginMaxAttempts`) instead of nested objects:
 * react-hook-form's `dirtyFields` gives an object for a nested object, not a
 * boolean, and flat fields map "which field changed" to "which key to send".
 */
export const throttleSettingsSchema = z.object({
  loginMaxAttempts: throttleAttempts(),
  loginWindowSecs: throttleWindow(),
  lookupMaxAttempts: throttleAttempts(),
  lookupWindowSecs: throttleWindow(),
  maxTrackedKeys: z
    .number()
    .int()
    .min(1, { error: () => m.settings_schema_at_least({ n: 1 }) })
    .max(MAX_TRACKED_KEYS_CEILING, {
      error: () =>
        m.settings_schema_max_value({ max: MAX_TRACKED_KEYS_CEILING }),
    }),
  pwnedMaxAttempts: throttleAttempts(),
  pwnedWindowSecs: throttleWindow(),
  registrationMaxAttempts: throttleAttempts(),
  registrationWindowSecs: throttleWindow(),
  resetMaxAttempts: throttleAttempts(),
  resetWindowSecs: throttleWindow(),
  shareMaxAttempts: throttleAttempts(),
  shareWindowSecs: throttleWindow(),
  sourceMaxAttempts: throttleAttempts(),
  sourceWindowSecs: throttleWindow(),
  speedMaxAttempts: throttleAttempts(),
  speedWindowSecs: throttleWindow(),
  verificationMaxAttempts: throttleAttempts(),
  verificationWindowSecs: throttleWindow(),
});

export type ThrottleSettingsValues = z.infer<typeof throttleSettingsSchema>;

// Passwords and hashing: auth.password.* / auth.hashing.* / auth.pwned.*

// The floor of `auth.password.min_length` and ceiling of `auth.password.max_length`
// are the same bounds the registration and password change forms check
// (`PasswordPolicy::FLOOR` / `CEILING`), defined in one place.

/** `AuthHashing::validate` in config.rs: below 7 MiB the hash is cheap to brute-force. */
export const MIN_MEMORY_KIB = 7 * 1024;

export const passwordSettingsSchema = z
  // oxlint-disable-next-line sort-keys -- length rules first, hashing cost after
  .object({
    // auth.password.min_length
    minLength: z
      .number()
      .int()
      .min(PASSWORD_MIN_FLOOR, {
        error: () => m.settings_schema_min_value({ min: PASSWORD_MIN_FLOOR }),
      })
      .max(PASSWORD_MAX_CEILING),
    // auth.password.max_length: an unbounded password field lets
    // unauthenticated requests make the server do arbitrary work
    maxLength: z
      .number()
      .int()
      .min(PASSWORD_MIN_FLOOR)
      .max(PASSWORD_MAX_CEILING, {
        error: () => m.settings_schema_max_value({ max: PASSWORD_MAX_CEILING }),
      }),
    // auth.password.require_mixed_case
    requireMixedCase: z.boolean(),
    // auth.password.require_digit
    requireDigit: z.boolean(),
    // auth.password.require_symbol
    requireSymbol: z.boolean(),
    // auth.hashing.memory_kib: the most important parameter; see the form's description
    memoryKib: z
      .number()
      .int()
      .min(MIN_MEMORY_KIB, {
        error: () => m.settings_schema_min_kib({ min: MIN_MEMORY_KIB }),
      }),
    // auth.hashing.iterations
    iterations: z
      .number()
      .int()
      .min(1, { error: () => m.settings_schema_at_least({ n: 1 }) }),
    // auth.hashing.lanes
    lanes: z
      .number()
      .int()
      .min(1, { error: () => m.settings_schema_at_least({ n: 1 }) }),
    // auth.pwned.enabled
    pwnedEnabled: z.boolean(),
    // auth.pwned.url: the server does not validate it while disabled, so neither does this
    pwnedUrl: z.string(),
  })
  .refine((values) => values.minLength <= values.maxLength, {
    error: () => m.settings_schema_password_length_order(),
    path: ["minLength"],
  })
  .refine(
    (values) => !values.pwnedEnabled || ABSOLUTE_URL.test(values.pwnedUrl),
    {
      error: () => m.settings_schema_pwned_url_required(),
      path: ["pwnedUrl"],
    }
  );

export type PasswordSettingsValues = z.infer<typeof passwordSettingsSchema>;

// Shares and retrieval: share.*

/** Range of `share.code_length` (`RetrievalCode::MIN_LENGTH`..=`MAX_LENGTH`). */
export const CODE_LENGTH_MIN = 4;
export const CODE_LENGTH_MAX = 32;

/** `INLINE_CEILING` in config.rs. */
export const MAX_INLINE_THRESHOLD_BYTES = 1024 * 1024;

/** `MAX_TEXT_CEILING` in config.rs. */
export const MAX_TEXT_BYTES = 16 * 1024 * 1024;

export const shareSettingsSchema = z
  // oxlint-disable-next-line sort-keys -- grouped by code / capacity / tickets / sweep
  .object({
    // share.code_length
    codeLength: z
      .number()
      .int()
      .min(CODE_LENGTH_MIN, {
        error: () => m.settings_schema_min_value({ min: CODE_LENGTH_MIN }),
      })
      .max(CODE_LENGTH_MAX, {
        error: () => m.settings_schema_max_value({ max: CODE_LENGTH_MAX }),
      }),
    // features.encryption
    encryption: z.boolean(),
    // share.blocked_extensions: string[] on the server, one per line in the form
    blockedExtensions: z.string(),
    // share.access_log.enabled
    accessLogEnabled: z.boolean(),
    // share.access_log.retention_days: cannot be 0 while enabled
    accessLogRetentionDays: z.number().int().min(0),
    // share.inline_threshold_bytes
    inlineThresholdBytes: z
      .number()
      .int()
      .min(0)
      .max(MAX_INLINE_THRESHOLD_BYTES, {
        error: () =>
          m.settings_schema_max_value({ max: MAX_INLINE_THRESHOLD_BYTES }),
      }),
    // share.max_text_bytes
    maxTextBytes: z
      .number()
      .int()
      .min(1, { error: () => m.settings_schema_at_least_bytes({ n: 1 }) })
      .max(MAX_TEXT_BYTES, {
        error: () => m.settings_schema_max_value({ max: MAX_TEXT_BYTES }),
      }),
    // share.ticket_ttl_hours
    ticketTtlHours: z
      .number()
      .int()
      .min(1, { error: () => m.settings_schema_at_least_hours({ n: 1 }) }),
    // share.ticket_idle_hours
    ticketIdleHours: z
      .number()
      .int()
      .min(1, { error: () => m.settings_schema_at_least_hours({ n: 1 }) }),
    // share.retain_after_death_hours: 0 means delete on the next sweep
    retainAfterDeathHours: z.number().int().min(0),
    // share.upload_session_ttl_hours
    uploadSessionTtlHours: z
      .number()
      .int()
      .min(1, { error: () => m.settings_schema_at_least_hours({ n: 1 }) }),
  })
  .refine(
    (values) => !values.accessLogEnabled || values.accessLogRetentionDays >= 1,
    {
      error: () => m.settings_schema_access_log_retention_min(),
      path: ["accessLogRetentionDays"],
    }
  );

export type ShareSettingsValues = z.infer<typeof shareSettingsSchema>;

// Feature switches: features.*
//
// `features.docs` is not here: it is FIXED_AT_BOOT (whether its routes are
// mounted is decided when building the router).

export const featureSettingsSchema = z.object({
  // features.accounts
  accounts: z.boolean(),
  // features.admin
  admin: z.boolean(),
});

export type FeatureSettingsValues = z.infer<typeof featureSettingsSchema>;

// Preview card: og.layout
//
// Layout rules (elements, attributes, size, external resources) are not checked
// here: only the server's `og::layout::check` is authoritative, and the editor
// asks it through `POST /admin/settings/preview`.

export const ogCardSettingsSchema = z.object({
  // og.layout: empty means the built-in card
  layout: z.string(),
});

export type OgCardSettingsValues = z.infer<typeof ogCardSettingsSchema>;
