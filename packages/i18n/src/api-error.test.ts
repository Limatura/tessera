import { describe, expect, it } from "vite-plus/test";

import { apiErrorCode, createApiErrorMessage } from "./api-error";
import type { ApiErrorMessages } from "./api-error";

/**
 * 假的訊息表：每個函式回傳「key + 參數」，測的是選哪一句、帶什麼參數，
 * 不是字串本身（字串在 messages/*.json，由 Paraglide 編譯）。
 */
const messages = new Proxy({} as ApiErrorMessages, {
  get: (_, key: string) => (inputs?: Record<string, unknown>) =>
    inputs ? `${key} ${JSON.stringify(inputs)}` : key,
});

const apiErrorMessage = createApiErrorMessage(messages);

function body(code: string, details: Record<string, unknown> = {}) {
  return { code, details, message: `server says ${code}` };
}

describe("apiErrorMessage", () => {
  it("把保留字的值帶進句子裡（#367 的例子）", () => {
    expect(apiErrorMessage(body("value_reserved", { value: "admin" }))).toBe(
      'api_error_value_reserved {"value":"admin"}'
    );
  });

  it("太短的密碼講出下限，欄位名稱也翻譯", () => {
    expect(
      apiErrorMessage(
        body("value_length", {
          actual: 7,
          field: "password",
          max: 1024,
          min: 12,
        })
      )
    ).toBe(
      'api_error_value_too_short {"field":"api_error_field_password","min":12}'
    );
  });

  it("太長的值講出上限", () => {
    expect(
      apiErrorMessage(
        body("value_length", { actual: 40, field: "username", max: 32, min: 3 })
      )
    ).toBe(
      'api_error_value_too_long {"field":"api_error_field_username","max":32}'
    );
  });

  it("只帶 max 的長度錯誤（分享標題、檔名）一定是太長", () => {
    expect(
      apiErrorMessage(body("value_length", { field: "title", max: 200 }))
    ).toBe(
      'api_error_value_too_long {"field":"api_error_field_title","max":200}'
    );
  });

  it("格式錯誤依 rule 給具體的句子", () => {
    expect(
      apiErrorMessage(
        body("value_invalid", { field: "password", rule: "needs_digit" })
      )
    ).toBe("api_error_rule_needs_digit");
  });

  it("不認得的 rule 退回「格式不正確」", () => {
    expect(
      apiErrorMessage(
        body("value_invalid", { field: "email", rule: "brand_new" })
      )
    ).toBe('api_error_value_invalid {"field":"api_error_field_email"}');
  });

  it("不認得的欄位不把 API 的欄位名放進句子", () => {
    expect(apiErrorMessage(body("value_empty", { field: "key_salt" }))).toBe(
      'api_error_value_empty {"field":"api_error_field_unknown"}'
    );
  });

  it("數值範圍依有哪些界限挑句子", () => {
    expect(
      apiErrorMessage(
        body("value_out_of_range", { field: "max_uses", max: 100, min: 1 })
      )
    ).toBe(
      'api_error_value_out_of_range_between {"field":"api_error_field_max_uses","max":100,"min":1}'
    );
    expect(
      apiErrorMessage(body("value_out_of_range", { field: "bytes", min: 0 }))
    ).toBe(
      'api_error_value_out_of_range_at_least {"field":"api_error_field_unknown","min":0}'
    );
  });

  it("等待時間不到一分鐘用秒，超過用分鐘（無條件進位）", () => {
    expect(
      apiErrorMessage(body("too_many_requests", { retry_after_secs: 30 }))
    ).toBe('api_error_too_many_requests_seconds {"seconds":30}');
    expect(
      apiErrorMessage(body("too_many_requests", { retry_after_secs: 90 }))
    ).toBe('api_error_too_many_requests_minutes {"minutes":2}');
    expect(apiErrorMessage(body("too_many_requests"))).toBe(
      "api_error_too_many_requests"
    );
  });

  it("位元組上限格式化成人看得懂的大小", () => {
    expect(
      apiErrorMessage(body("payload_too_large", { limit_bytes: 1_048_576 }))
    ).toBe('api_error_payload_too_large_limit {"limit":"1.0 MB"}');
  });

  it("儲存空間依 tier 分開講", () => {
    expect(
      apiErrorMessage(body("insufficient_storage", { tier: "role" }))
    ).toBe("api_error_insufficient_storage_role");
  });

  it("原因只在 message 裡的 code，把 message 接在翻譯後面", () => {
    expect(
      apiErrorMessage({
        code: "provider_unreachable",
        details: {},
        message: "connection refused",
      })
    ).toBe('api_error_provider_unreachable {"reason":"connection refused"}');
  });

  it("前端還不認得的 code 退回 server 的 message", () => {
    expect(apiErrorMessage(body("added_next_release"))).toBe(
      "server says added_next_release"
    );
  });

  it("沒有 code（斷線、前端自己丟的 Error）用 fallback，不顯示內部字串", () => {
    expect(apiErrorMessage(new TypeError("Failed to fetch"), "try again")).toBe(
      "try again"
    );
    expect(apiErrorMessage(null)).toBe("api_error_unknown");
  });

  it("details 缺漏或型別不對也不會拋錯", () => {
    expect(
      apiErrorMessage({ code: "value_length", details: null, message: "x" })
    ).toBe('api_error_value_length {"field":"api_error_field_unknown"}');
    expect(
      apiErrorMessage(body("value_length", { field: "password", min: "12" }))
    ).toBe('api_error_value_length {"field":"api_error_field_password"}');
  });

  it("上傳時丟的 Error 掛著 code 也能翻譯", () => {
    const error = Object.assign(new Error("that upload was abandoned"), {
      code: "upload_abandoned",
      details: {},
    });
    expect(apiErrorMessage(error)).toBe("api_error_upload_abandoned");
  });
});

describe("apiErrorCode", () => {
  it("讀出 code，不是物件或沒有 code 時回 undefined", () => {
    expect(apiErrorCode(body("password_breached"))).toBe("password_breached");
    expect(apiErrorCode(new Error("nope"))).toBeUndefined();
    expect(apiErrorCode("value_empty")).toBeUndefined();
  });
});
