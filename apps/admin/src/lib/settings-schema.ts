import {
  APPEARANCE_ACCENTS,
  APPEARANCE_MODES,
  APPEARANCE_THEMES,
} from "@tessera/ui/lib/appearance";
import { z } from "zod";

/**
 * 表單欄位對應到 `apps/server/SETTINGS.md` 列出的 dotted key（`settings::keys()`
 * 的實際輸出），值轉成 camelCase。
 *
 * 三種欄位這裡不收：
 *
 * - `FIXED_AT_BOOT`（`server.max_body_bytes`...）—— PUT 會被拒，而 settings 是整批
 *   驗證，混進去會連同一批的合法欄位一起退回。要顯示就用唯讀欄位。
 * - `SECRET`（`storage.url`...）—— 讀不到也寫不了，只能用環境變數。
 * - `SEALED`（`*.client_secret`、`mail.username` / `mail.password`）—— 寫得進去、讀不
 *   回來。欄位收在這裡，但語意是「留空 = 不改動，有填 = 覆寫」，不是目前的值。
 */

// ---- 外觀：appearance.* ----
//
// 三個 enum 住在 `@tessera/ui/lib/appearance`，跟它們對應的 globals.css 放在一起；
// 那份清單同時要跟 `crates/tessera-core/src/appearance.rs` 一致。這裡只是轉出去，
// 讓本檔仍是「表單欄位形狀」的單一入口。
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

// ---- 站台資訊：site.* ----

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

export const siteSettingsSchema = z.object({
  // site.description —— 空 = 不輸出這個 meta
  description: z
    .string()
    .refine((value) => codePoints(value) <= SITE_DESCRIPTION_MAX, {
      error: `最多 ${SITE_DESCRIPTION_MAX} 字`,
    }),
  // site.title
  title: z
    .string()
    .refine((value) => value.trim() !== "", { error: "站名不可空白" })
    .refine((value) => codePoints(value) <= SITE_TITLE_MAX, {
      error: `最多 ${SITE_TITLE_MAX} 字`,
    }),
});

export type SiteSettingsValues = z.infer<typeof siteSettingsSchema>;

// ---- 存儲配置：server.part_size_bytes ----
//
// `storage.url` 不在這裡：它是 SECRET，URL 帶帳密，面板讀不到也改不了。

/** S3 拒絕最後一片以外小於 5 MiB 的 part。 */
export const MIN_PART_SIZE_BYTES = 5 * 1024 * 1024;

export const storageSettingsSchema = z.object({
  // server.part_size_bytes
  partSizeBytes: z
    .number()
    .int()
    .min(MIN_PART_SIZE_BYTES, { error: "至少需要 5 MiB（S3 分片下限）" }),
});

export type StorageSettingsValues = z.infer<typeof storageSettingsSchema>;

// ---- 上傳設置：auth.throttle.source.* + features.anonymous_upload
//      + storage.max_total_bytes + share.default_expiry_hours / max_expiry_hours ----
//
// `server.max_body_bytes` 不在這裡：它是 FIXED_AT_BOOT，只能唯讀顯示。
const BYTES_MIN = 0;

export const uploadSettingsSchema = z
  // oxlint-disable-next-line sort-keys -- 依限流／容量／保存時間的敘事順序分組，字母序會打散語意
  .object({
    // auth.throttle.source.window_secs
    throttleWindowSecs: z.number().int().min(1, { error: "至少 1 秒" }),
    // auth.throttle.source.max_attempts
    throttleMaxAttempts: z.number().int().min(1, { error: "至少 1 次" }),
    // features.anonymous_upload
    allowAnonymousUpload: z.boolean(),
    // storage.max_total_bytes
    maxTotalBytes: z.number().int().min(BYTES_MIN),
    // share.default_expiry_hours，0 代表使用 maxExpiryHours
    defaultExpiryHours: z.number().int().min(0),
    // share.max_expiry_hours，0 代表無上限
    maxExpiryHours: z.number().int().min(0),
  })
  .refine(
    (values) =>
      values.maxExpiryHours === 0 ||
      values.defaultExpiryHours === 0 ||
      values.defaultExpiryHours <= values.maxExpiryHours,
    {
      error: "預設保存時間不可超過最長保存時間",
      path: ["defaultExpiryHours"],
    }
  );

export type UploadSettingsValues = z.infer<typeof uploadSettingsSchema>;

// ---- 帳號與註冊：auth.registration / auth.default_role
//      / auth.session_ttl_hours / auth.login_history_days ----

/** 對應 server 的 `auth.registration`，值原樣送出。 */
export const REGISTRATION_MODES = ["open", "invite", "closed"] as const;

export const accountSettingsSchema = z.object({
  // auth.default_role — 角色「名稱」不是 id，選項來自 GET /admin/roles
  defaultRole: z.string().min(1, { error: "請選擇一個角色" }),
  // auth.login_history_days
  loginHistoryDays: z.number().int().min(1, { error: "至少 1 天" }),
  // auth.registration
  registration: z.enum(REGISTRATION_MODES),
  // auth.session_ttl_hours
  sessionTtlHours: z.number().int().min(1, { error: "至少 1 小時" }),
});

export type AccountSettingsValues = z.infer<typeof accountSettingsSchema>;

// ---- 登入方式：auth.oidc.* / auth.github.* / auth.discord.* ----

const HTTPS_URL = /^https:\/\/[^\s/][^\s]*$/u;
const ABSOLUTE_URL = /^https?:\/\/[^\s/][^\s]*$/u;

/**
 * OIDC。空的 `issuer` = 整個功能關閉，所以沒有另外的「啟用」開關 —— 多一個布林
 * 只會讓「填了 issuer 但忘了打開」變成一種可能的狀態。
 */
export const oidcSettingsSchema = z.object({
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
      error: "必須是 https:// 開頭的 issuer URL",
    })
    /*
     * 貼成 discovery URL 是最常見的填法錯誤，而且它會通過上面那條（它也是合法
     * 的 https URL）。後端抓取時自己會接 `/.well-known/openid-configuration`，
     * 所以存成這樣的結果是每次登入都以 `unavailable` 失敗 —— 擋在儲存前，
     * 比讓人事後從一個錯誤代碼回推便宜得多。
     */
    .refine((value) => !value.includes("/.well-known/"), {
      error:
        "這是 discovery URL，不是 issuer。把它貼到上面那格按「讀取並帶入」，issuer 會從文件裡讀出來",
    }),
  // auth.oidc.link_by_verified_email
  linkByVerifiedEmail: z.boolean(),
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
  // auth.{github,discord}.base_url
  baseUrl: z
    .string()
    .refine((value) => value === "" || ABSOLUTE_URL.test(value), {
      error: "必須是 http(s):// 開頭的絕對位址",
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

// ---- 寄信：mail.* ----

/** `mail.reset_ttl_minutes` 的上限（24 小時），對應 config.rs 的同名常數。 */
export const MAX_RESET_TTL_MINUTES = 24 * 60;

/** `mail.registration_ttl_hours` 的上限（7 天）。 */
export const MAX_REGISTRATION_TTL_HOURS = 7 * 24;

const MAX_PORT = 65_535;

/** `host` / `from` / `public_url` 這組要嘛全填要嘛全空，server 拒絕半套設定。 */
const MAIL_TRIO = ["host", "from", "publicUrl"] as const;

export const mailSettingsSchema = z
  // oxlint-disable-next-line sort-keys -- 依連線／寄件人／時效的敘事順序分組
  .object({
    // mail.host —— 空字串關閉所有寄信功能
    host: z.string(),
    // mail.port
    port: z.number().int().min(1, { error: "至少 1" }).max(MAX_PORT),
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
      .min(1, { error: "至少 1 分鐘" })
      .max(MAX_RESET_TTL_MINUTES, { error: "上限 1440 分鐘（24 小時）" }),
    // mail.registration_ttl_hours
    registrationTtlHours: z
      .number()
      .int()
      .min(1, { error: "至少 1 小時" })
      .max(MAX_REGISTRATION_TTL_HOURS, { error: "上限 168 小時（7 天）" }),
  })
  .superRefine((values, ctx) => {
    if (values.from.trim() !== "" && !values.from.includes("@")) {
      ctx.addIssue({
        code: "custom",
        message: "請輸入完整的寄件位址",
        path: ["from"],
      });
    }

    if (
      values.publicUrl.trim() !== "" &&
      !ABSOLUTE_URL.test(values.publicUrl)
    ) {
      ctx.addIssue({
        code: "custom",
        message: "必須是 http(s):// 開頭的絕對位址",
        path: ["publicUrl"],
      });
    }

    // 半套會被 server 拒絕，而整批驗證會讓同一次送出的其他欄位一起退回，
    // 所以在這裡就擋下來並指出缺的是哪一格。
    const missing = MAIL_TRIO.filter((name) => values[name].trim() === "");
    if (missing.length === 0 || missing.length === MAIL_TRIO.length) {
      return;
    }

    for (const name of missing) {
      ctx.addIssue({
        code: "custom",
        message: "SMTP 主機、寄件位址、信件連結基底要嘛全填、要嘛全空",
        path: [name],
      });
    }
  });

export type MailSettingsValues = z.infer<typeof mailSettingsSchema>;

// ---- 網路與反向代理：server.* ----

/** 對應 server 的 `auth.cookie_secure`。 */
export const COOKIE_SECURE_POLICIES = ["auto", "always", "never"] as const;

export const networkSettingsSchema = z
  // oxlint-disable-next-line sort-keys -- 依對外位址／代理／逾時的敘事順序分組
  .object({
    // server.public_url —— 必須絕對，結尾斜線會被 server 去掉
    publicUrl: z
      .string()
      .refine((value) => value === "" || ABSOLUTE_URL.test(value), {
        error: "必須是 http(s):// 開頭的絕對位址",
      }),
    // server.trusted_proxies —— 後端是 string[]，表單用一行一筆
    trustedProxies: z.string(),
    // auth.cookie_secure
    cookieSecure: z.enum(COOKIE_SECURE_POLICIES),
    // server.request_timeout_secs —— 不套用在上傳路由
    requestTimeoutSecs: z.number().int().min(1, { error: "至少 1 秒" }),
    // server.upload_stall_secs
    uploadStallSecs: z.number().int().min(1, { error: "至少 1 秒" }),
  });

export type NetworkSettingsValues = z.infer<typeof networkSettingsSchema>;

// ---- 限流：auth.throttle.* ----

/**
 * 六組計數器保護不同東西，每組都是 `{max_attempts, window_secs}`。
 *
 * 兩個值都不能是 0，server 啟動時會拒絕：0 次嘗試等於鎖死所有人（在 `source`
 * 上就是整台機器），0 秒視窗則永遠不會擋，看起來開著實際上關著。
 */
export const THROTTLE_COUNTERS = [
  "login",
  "source",
  "lookup",
  "reset",
  "registration",
  "share",
] as const;

export type ThrottleCounter = (typeof THROTTLE_COUNTERS)[number];

/** `auth.throttle.max_tracked_keys` 的上限。這是記憶體上限不是安全設定。 */
export const MAX_TRACKED_KEYS_CEILING = 100_000;

function throttleAttempts() {
  return z.number().int().min(1, { error: "至少 1 次，0 會鎖死所有人" });
}

function throttleWindow() {
  return z.number().int().min(1, { error: "至少 1 秒，0 等於完全不限流" });
}

/**
 * 欄位刻意攤平成 `loginMaxAttempts` 這種名字而不是巢狀物件：送出時只挑 dirty 的
 * 欄位，而 react-hook-form 的 `dirtyFields` 對巢狀物件給的是物件而非布林，攤平讓
 * 「哪一格動過」跟「要送哪個 key」一對一。
 */
export const throttleSettingsSchema = z.object({
  loginMaxAttempts: throttleAttempts(),
  loginWindowSecs: throttleWindow(),
  lookupMaxAttempts: throttleAttempts(),
  lookupWindowSecs: throttleWindow(),
  maxTrackedKeys: z
    .number()
    .int()
    .min(1, { error: "至少 1" })
    .max(MAX_TRACKED_KEYS_CEILING, { error: "上限 100000" }),
  registrationMaxAttempts: throttleAttempts(),
  registrationWindowSecs: throttleWindow(),
  resetMaxAttempts: throttleAttempts(),
  resetWindowSecs: throttleWindow(),
  shareMaxAttempts: throttleAttempts(),
  shareWindowSecs: throttleWindow(),
  sourceMaxAttempts: throttleAttempts(),
  sourceWindowSecs: throttleWindow(),
});

export type ThrottleSettingsValues = z.infer<typeof throttleSettingsSchema>;

// ---- 密碼與雜湊：auth.password.* / auth.hashing.* ----

/** `auth.password.min_length` 的下限（`PasswordPolicy::FLOOR`）。 */
export const PASSWORD_MIN_FLOOR = 8;

/** `auth.password.max_length` 的上限（`PasswordPolicy::CEILING`）。 */
export const PASSWORD_MAX_CEILING = 1024;

export const passwordSettingsSchema = z
  // oxlint-disable-next-line sort-keys -- 長度規則先，雜湊成本後
  .object({
    // auth.password.min_length
    minLength: z
      .number()
      .int()
      .min(PASSWORD_MIN_FLOOR, { error: "不可低於 8" })
      .max(PASSWORD_MAX_CEILING),
    // auth.password.max_length —— 無上限的密碼欄位等於讓未認證請求叫伺服器做任意多的工
    maxLength: z
      .number()
      .int()
      .min(PASSWORD_MIN_FLOOR)
      .max(PASSWORD_MAX_CEILING, { error: "上限 1024" }),
    // auth.password.require_mixed_case
    requireMixedCase: z.boolean(),
    // auth.password.require_digit
    requireDigit: z.boolean(),
    // auth.password.require_symbol
    requireSymbol: z.boolean(),
    // auth.hashing.memory_kib —— 最關鍵的參數，見表單說明
    memoryKib: z.number().int().min(8, { error: "至少 8 KiB" }),
    // auth.hashing.iterations
    iterations: z.number().int().min(1, { error: "至少 1" }),
    // auth.hashing.lanes
    lanes: z.number().int().min(1, { error: "至少 1" }),
  })
  .refine((values) => values.minLength <= values.maxLength, {
    error: "最短長度不可大於最長長度",
    path: ["minLength"],
  });

export type PasswordSettingsValues = z.infer<typeof passwordSettingsSchema>;

// ---- 分享與取件：share.* ----

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
      .min(CODE_LENGTH_MIN, { error: "至少 4 碼" })
      .max(CODE_LENGTH_MAX, { error: "最多 32 碼" }),
    // share.access_log.enabled
    accessLogEnabled: z.boolean(),
    // share.access_log.retention_days —— 開啟時不可為 0
    accessLogRetentionDays: z.number().int().min(0),
    // share.inline_threshold_bytes
    inlineThresholdBytes: z.number().int().min(0),
    // share.max_text_bytes
    maxTextBytes: z.number().int().min(1, { error: "至少 1 byte" }),
    // share.ticket_ttl_hours
    ticketTtlHours: z.number().int().min(1, { error: "至少 1 小時" }),
    // share.ticket_idle_hours
    ticketIdleHours: z.number().int().min(1, { error: "至少 1 小時" }),
    // share.retain_after_death_hours —— 0 代表下次清掃就刪
    retainAfterDeathHours: z.number().int().min(0),
    // share.upload_session_ttl_hours
    uploadSessionTtlHours: z.number().int().min(1, { error: "至少 1 小時" }),
  })
  .refine(
    (values) => !values.accessLogEnabled || values.accessLogRetentionDays >= 1,
    {
      error: "存取紀錄開啟時，保留天數至少 1 天（沒有「永久保留」這個值）",
      path: ["accessLogRetentionDays"],
    }
  );

export type ShareSettingsValues = z.infer<typeof shareSettingsSchema>;

// ---- 功能開關與預覽卡片：features.* / og.layout ----
//
// `features.docs` 不在這裡：它是 FIXED_AT_BOOT（建路由時決定要不要掛）。

export const featureSettingsSchema = z.object({
  // features.accounts
  accounts: z.boolean(),
  // features.admin
  admin: z.boolean(),
  // og.layout —— 只允許 div / span / style，且不能指向本伺服器以外的資源
  ogLayout: z.string(),
});

export type FeatureSettingsValues = z.infer<typeof featureSettingsSchema>;
