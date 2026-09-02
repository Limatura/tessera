import { z } from "zod";

/**
 * 表單欄位對應到 apps/server/src/config.rs 的 `Config`。命名沿用該檔案的
 * snake_case 分組（server.*、storage.*、share.* ...），值轉成 camelCase。
 *
 * 標了「(新)」的 section／欄位是 server 目前還沒有的設定
 * （OIDC、網頁標題/敘述/SEO、主題、主頁管理入口顯示、允許上傳類型），
 * server 尚未提供對應的 config 欄位與 API，之後補上時再對齊。
 */

// ---- OIDC 設定（新，server 尚無對應欄位）----
export const oidcSettingsSchema = z.object({
  clientId: z.string(),
  clientSecret: z.string(),
  enabled: z.boolean(),
  issuerUrl: z.union([z.url({ error: "請輸入有效的 URL" }), z.literal("")]),
  scopes: z.string(),
});

export type OidcSettingsValues = z.infer<typeof oidcSettingsSchema>;

// ---- 網頁基本資訊（新，server 尚無對應欄位）----
export const siteSettingsSchema = z.object({
  description: z.string().max(500),
  seoKeywords: z.string().max(300),
  showAdminEntryOnHome: z.boolean(),
  title: z.string().min(1, { error: "網頁標題不可為空" }).max(100),
});

export type SiteSettingsValues = z.infer<typeof siteSettingsSchema>;

// ---- 界面主題（新，server 尚無對應欄位）----
export const themeSettingsSchema = z.object({
  accentColor: z.string(),
  mode: z.enum(["system", "light", "dark"]),
});

export type ThemeSettingsValues = z.infer<typeof themeSettingsSchema>;

// ---- 存儲配置：storage.url + server.part_size_bytes ----
export const storageSettingsSchema = z.object({
  // storage.url（本機路徑或 s3://、gs://... 等 object_store URL）
  backendUrl: z.string().min(1, { error: "請輸入儲存路徑或 URL" }),
  // server.part_size_bytes；S3 分片下限 5 MiB
  partSizeBytes: z
    .number()
    .int()
    .min(5 * 1024 * 1024, { error: "至少需要 5 MiB（S3 分片下限）" }),
});

export type StorageSettingsValues = z.infer<typeof storageSettingsSchema>;

// ---- 上傳設置：throttle.source（限流視窗/次數）+ server.max_body_bytes
//      + 允許上傳類型（新）+ features.anonymous_upload
//      + storage.max_total_bytes + share.default_expiry_hours/max_expiry_hours ----
const BYTES_MIN = 0;

export const uploadSettingsSchema = z
  // oxlint-disable-next-line sort-keys -- 依限流／單檔上限／保存時間的敘事順序分組，字母序會打散語意
  .object({
    // auth.throttle.source.window_secs
    throttleWindowSecs: z.number().int().min(1, { error: "至少 1 秒" }),
    // auth.throttle.source.max_attempts
    throttleMaxAttempts: z.number().int().min(1, { error: "至少 1 次" }),
    // server.max_body_bytes
    maxBodyBytes: z.number().int().min(1, { error: "至少 1 byte" }),
    // 允許上傳類型（新）：逗號分隔的副檔名或 MIME，空白代表不限制
    allowedFileTypes: z.string(),
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
