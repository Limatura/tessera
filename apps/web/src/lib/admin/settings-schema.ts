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
 * 表單欄位對應 `apps/server/SETTINGS.md` 列出的 dotted key，值轉成 camelCase。
 *
 * 三種欄位這裡不收：
 * - `FIXED_AT_BOOT`（如 `server.max_body_bytes`）—— PUT 會被拒整批，改用唯讀欄位顯示。
 * - `SECRET`（如 `scan.authorization`）—— 讀不到也寫不了，只能走專用端點。
 * - `SEALED`（如 `*.client_secret`、`mail.password`）—— 寫得進去讀不回來，留空 = 不改動。
 *
 * 錯誤訊息一律用 `error: () => m.xxx()`：schema 是模組載入時建一次的常數，直接呼叫
 * `m.xxx()` 會把當下語系烤進字串，包一層 function 才能跟著切換語言走。
 */

// 外觀：appearance.*
// 三個 enum 住在 `@tessera/ui/lib/appearance`，需跟 `appearance.rs` 保持一致，這裡只轉出去。
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

// 站台資訊：site.*

/** `site.title` 的上限（`SiteConfig::MAX_TITLE`）。 */
export const SITE_TITLE_MAX = 100;

/** `site.description` 的上限（`SiteConfig::MAX_DESCRIPTION`）。 */
export const SITE_DESCRIPTION_MAX = 300;

/**
 * server 用 `chars().count()` 算長度，也就是 Unicode code point。`.length` 算的是
 * UTF-16 code unit，emoji 之類會被算成兩個，前端會比後端先拒絕。
 */
function codePoints(value: string): number {
  return [...value].length;
}

/**
 * `site.language`：分享連結預覽（meta 標籤、`og.png` 卡片）上的字用哪個語言。
 * 爬蟲不帶語言偏好，所以用站台的。不影響網頁介面本身的語言。
 */
export const SITE_LANGUAGES = ["en", "zh-TW", "ja"] as const;

export const siteSettingsSchema = z.object({
  // site.description —— 空 = 不輸出這個 meta
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

// 儲存空間：server.part_size_bytes + storage.split_across_backends
//      + storage.reconcile_interval_hours / reconcile_grace_hours
//
// 儲存位置不是設定：每個 backend 是 `storage_backends` 表裡的一列，從
// `/admin/storage/backends` 管，不透過這裡的 `PUT /admin/settings`。

/** S3 拒絕最後一片以外小於 5 MiB 的 part。 */
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
  // storage.reconcile_interval_hours —— 0 = 不對帳
  reconcileIntervalHours: z.number().int().min(0),
  // storage.split_across_backends
  splitAcrossBackends: z.boolean(),
});

export type StorageSettingsValues = z.infer<typeof storageSettingsSchema>;

// 上傳與保存：features.anonymous_upload + storage.max_total_bytes
//      + share.default_expiry_hours / max_expiry_hours
//
// `server.max_body_bytes` 不在這裡：它是 FIXED_AT_BOOT，只能唯讀顯示。
// `auth.throttle.source.*` 以前放在這一頁，但它管的是同一個來源的所有昂貴操作，
// 不只上傳（#387），所以搬到限流頁。
const BYTES_MIN = 0;

export const uploadSettingsSchema = z
  // oxlint-disable-next-line sort-keys -- 依誰能傳／保存時間／容量的敘事順序分組，字母序會打散語意
  .object({
    // features.anonymous_upload
    allowAnonymousUpload: z.boolean(),
    // share.default_expiry_hours，0 代表使用 maxExpiryHours
    defaultExpiryHours: z.number().int().min(0),
    // share.max_expiry_hours，0 代表無上限
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

// 帳號與註冊：auth.registration / auth.default_role
//      / auth.session_ttl_hours / auth.login_history_days

/** 對應 server 的 `auth.registration`，值原樣送出。 */
export const REGISTRATION_MODES = ["open", "invite", "closed"] as const;

export const accountSettingsSchema = z.object({
  // auth.default_role — 角色「名稱」不是 id，選項來自 GET /admin/roles
  defaultRole: z
    .string()
    .min(1, { error: () => m.settings_schema_select_role() }),
  // auth.login_history_days
  loginHistoryDays: z
    .number()
    .int()
    .min(1, { error: () => m.settings_schema_at_least_days({ n: 1 }) }),
  // auth.registration
  registration: z.enum(REGISTRATION_MODES),
  // auth.session_ttl_hours
  sessionTtlHours: z
    .number()
    .int()
    .min(1, { error: () => m.settings_schema_at_least_hours({ n: 1 }) }),
});

export type AccountSettingsValues = z.infer<typeof accountSettingsSchema>;

// 登入方式：auth.oidc.* / auth.github.* / auth.discord.*

/** 對應 server 的 `ProviderRegistration`：每家第三方登入要不要開新帳號。 */
export const PROVIDER_REGISTRATION_MODES = [
  "inherit",
  "always",
  "never",
] as const;

export type ProviderRegistrationMode =
  (typeof PROVIDER_REGISTRATION_MODES)[number];

/** 對應 server 的 `LinkByEmail`，只有 OIDC 有三段；GitHub / Discord 仍是開關。 */
export const LINK_BY_EMAIL_MODES = [
  "never",
  "when_verified",
  "always",
] as const;

const HTTPS_URL = /^https:\/\/[^\s/][^\s]*$/u;
const ABSOLUTE_URL = /^https?:\/\/[^\s/][^\s]*$/u;

/**
 * OIDC。空的 `issuer` = 整個功能關閉，所以沒有另外的「啟用」開關 —— 多一個布林
 * 只會讓「填了 issuer 但忘了打開」變成一種可能的狀態。
 */
export const oidcSettingsSchema = z.object({
  // auth.oidc.allow_registration
  allowRegistration: z.enum(PROVIDER_REGISTRATION_MODES),
  // auth.oidc.client_id
  clientId: z.string(),
  // auth.oidc.client_secret（SEALED，留空 = 不改動）
  clientSecret: z.string(),
  // auth.oidc.display_name
  displayName: z.string(),
  // auth.oidc.group_claim
  groupClaim: z.string(),
  // auth.oidc.issuer —— 規範要求與 `iss` claim 完全字串相等，且必須 https
  issuer: z
    .string()
    .refine((value) => value === "" || HTTPS_URL.test(value), {
      error: () => m.settings_schema_oidc_issuer_https(),
    })
    // 貼成 discovery URL 是常見填法錯誤，會通過上一條檢查，但後端會自己接 `/.well-known/...`，每次登入都失敗
    .refine((value) => !value.includes("/.well-known/"), {
      error: () => m.settings_schema_oidc_issuer_not_discovery_url(),
    }),
  // auth.oidc.link_by_email
  linkByEmail: z.enum(LINK_BY_EMAIL_MODES),
  // auth.oidc.required_group —— 空字串 = 該 provider 認證過的任何人都能登入
  requiredGroup: z.string(),
  // auth.oidc.scopes —— server 是 string[]，表單用空白分隔的字串
  scopes: z.string(),
});

export type OidcSettingsValues = z.infer<typeof oidcSettingsSchema>;

/**
 * GitHub 與 Discord。兩邊欄位相同，只有 `baseUrl` 僅 GitHub 用得到
 * （GitHub Enterprise Server），Discord 的卡片不畫那一格也不送那個 key。
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
  // auth.{github,discord}.client_secret（SEALED，留空 = 不改動）
  clientSecret: z.string(),
  // auth.{github,discord}.display_name
  displayName: z.string(),
  // auth.{github,discord}.link_by_verified_email
  linkByVerifiedEmail: z.boolean(),
  // auth.{github,discord}.scopes
  scopes: z.string(),
});

export type OauthSettingsValues = z.infer<typeof oauthSettingsSchema>;

// 寄信：mail.*

/** `mail.reset_ttl_minutes` 的上限（24 小時），對應 config.rs 的同名常數。 */
export const MAX_RESET_TTL_MINUTES = 24 * 60;

/** `mail.registration_ttl_hours` 的上限（7 天）。 */
export const MAX_REGISTRATION_TTL_HOURS = 7 * 24;

const MAX_PORT = 65_535;

/** `host` / `from` / `public_url` 這組要嘛全填要嘛全空，server 拒絕半套設定。 */
const MAIL_TRIO = ["host", "from", "publicUrl"] as const;

/** 對應 server 的 `mail.tls`。`none` 只給本機/內網 relay 用，見表單警告。 */
export const MAIL_TLS_MODES = ["auto", "starttls", "implicit", "none"] as const;

export const mailSettingsSchema = z
  // oxlint-disable-next-line sort-keys -- 依連線／寄件人／時效的敘事順序分組
  .object({
    // mail.host —— 空字串關閉所有寄信功能
    host: z.string(),
    // mail.port
    port: z
      .number()
      .int()
      .min(1, { error: () => m.settings_schema_at_least({ n: 1 }) })
      .max(MAX_PORT),
    // mail.tls
    tls: z.enum(MAIL_TLS_MODES),
    // mail.username（SEALED，留空 = 不改動）
    username: z.string(),
    // mail.password（SEALED，留空 = 不改動）
    password: z.string(),
    // mail.from
    from: z.string(),
    // mail.public_url —— 信件內連結的基底，刻意不從請求的 Host header 推導
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

    // 半套會被 server 拒整批，連帶退回其他合法欄位，所以先在這裡擋下並指出缺哪一格
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

// 網路與反向代理：server.*

/** 對應 server 的 `auth.cookie_secure`。 */
export const COOKIE_SECURE_POLICIES = ["auto", "always", "never"] as const;

export const networkSettingsSchema = z
  // oxlint-disable-next-line sort-keys -- 依對外位址／代理／逾時的敘事順序分組
  .object({
    // server.public_url —— 必須絕對，結尾斜線會被 server 去掉
    publicUrl: z
      .string()
      .refine((value) => value === "" || ABSOLUTE_URL.test(value), {
        error: () => m.settings_schema_absolute_url(),
      }),
    // server.trusted_proxies —— 後端是 string[]，表單用一行一筆
    trustedProxies: z.string(),
    // auth.cookie_secure
    cookieSecure: z.enum(COOKIE_SECURE_POLICIES),
    // server.request_timeout_secs —— 不套用在上傳路由
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

// 限流：auth.throttle.*

/**
 * 九組計數器保護不同東西，都是 `{max_attempts, window_secs}`。兩個值不能是 0——
 * 0 次嘗試等於鎖死所有人（`source` 等於鎖死整台機器），0 秒視窗則永遠不擋。
 * 順序即畫面順序：影響最大的 `source` 放最前，其餘依保護對象分組。
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

/** `auth.throttle.max_tracked_keys` 的上限。這是記憶體上限不是安全設定。 */
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
 * 欄位刻意攤平成 `loginMaxAttempts` 而非巢狀物件：react-hook-form 的
 * `dirtyFields` 對巢狀物件給的是物件而非布林，攤平才能讓「哪格動過」對應「送哪個 key」。
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

// 密碼與雜湊：auth.password.* / auth.hashing.* / auth.pwned.*

// `auth.password.min_length` 的下限、`auth.password.max_length` 的上限，跟註冊／
// 改密碼表單驗的是同一組（`PasswordPolicy::FLOOR` / `CEILING`），只定義在一處。

export const passwordSettingsSchema = z
  // oxlint-disable-next-line sort-keys -- 長度規則先，雜湊成本後
  .object({
    // auth.password.min_length
    minLength: z
      .number()
      .int()
      .min(PASSWORD_MIN_FLOOR, {
        error: () => m.settings_schema_min_value({ min: PASSWORD_MIN_FLOOR }),
      })
      .max(PASSWORD_MAX_CEILING),
    // auth.password.max_length —— 無上限的密碼欄位等於讓未認證請求叫伺服器做任意多的工
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
    // auth.hashing.memory_kib —— 最關鍵的參數，見表單說明
    memoryKib: z
      .number()
      .int()
      .min(8, { error: () => m.settings_schema_min_kib({ min: 8 }) }),
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
    // auth.pwned.url —— 關閉時 server 不驗證，所以這裡也不擋
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

// 分享與取件：share.*

/** `share.code_length` 的範圍（`RetrievalCode::MIN_LENGTH`..=`MAX_LENGTH`）。 */
export const CODE_LENGTH_MIN = 4;
export const CODE_LENGTH_MAX = 32;

export const shareSettingsSchema = z
  // oxlint-disable-next-line sort-keys -- 依取件碼／容量／授權／清掃的敘事順序分組
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
    // share.access_log.enabled
    accessLogEnabled: z.boolean(),
    // share.access_log.retention_days —— 開啟時不可為 0
    accessLogRetentionDays: z.number().int().min(0),
    // share.inline_threshold_bytes
    inlineThresholdBytes: z.number().int().min(0),
    // share.max_text_bytes
    maxTextBytes: z
      .number()
      .int()
      .min(1, { error: () => m.settings_schema_at_least_bytes({ n: 1 }) }),
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
    // share.retain_after_death_hours —— 0 代表下次清掃就刪
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

// 功能開關：features.*
//
// `features.docs` 不在這裡：它是 FIXED_AT_BOOT（建路由時決定要不要掛）。

export const featureSettingsSchema = z.object({
  // features.accounts
  accounts: z.boolean(),
  // features.admin
  admin: z.boolean(),
});

export type FeatureSettingsValues = z.infer<typeof featureSettingsSchema>;

// 預覽卡片：og.layout
//
// 版面規則(元素、屬性、大小、外部資源)不在這裡驗:只有 server 的 `og::layout::check`
// 是準的,編輯器打 `POST /admin/settings/preview` 去問它。

export const ogCardSettingsSchema = z.object({
  // og.layout —— 空 = 內建卡片
  layout: z.string(),
});

export type OgCardSettingsValues = z.infer<typeof ogCardSettingsSchema>;
