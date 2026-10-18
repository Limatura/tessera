import { describe, expect, it, vi } from "vite-plus/test";

import { apiErrorCode, apiErrorMessage } from "./api-error";

/**
 * Fake messages: each function returns its key and arguments. The tests check
 * which sentence is chosen and with what arguments, not the strings themselves
 * (those are in messages/*.json, compiled by Paraglide).
 *
 * `vi.hoisted`: `vi.mock` runs before the module's own top-level code.
 */
const { fakeMessage, isMessage } = vi.hoisted(() => ({
  fakeMessage: (key: string) => (inputs?: Record<string, unknown>) =>
    inputs ? `${key} ${JSON.stringify(inputs)}` : key,
  // Only message keys: anything else (`then`, symbols) must stay undefined,
  // or the module looks like a promise.
  isMessage: (key: string | symbol): key is string =>
    typeof key === "string" &&
    (key.startsWith("api_error_") || key.startsWith("shares_")),
}));

vi.mock(
  "@/paraglide/messages",
  () =>
    new Proxy(
      {},
      {
        get: (_, key) => (isMessage(key) ? fakeMessage(key) : undefined),
        has: (_, key) => isMessage(key),
      }
    )
);

function body(code: string, details: Record<string, unknown> = {}) {
  return { code, details, message: `server says ${code}` };
}

describe("apiErrorMessage", () => {
  it("a server error ends the sentence with its request id", () => {
    expect(
      apiErrorMessage({ ...body("internal"), request_id: "01JREQUEST" })
    ).toBe(
      'api_error_with_request_id {"message":"api_error_internal","requestId":"01JREQUEST"}'
    );
  });

  it("without a request id the sentence is unchanged", () => {
    expect(apiErrorMessage(body("internal"))).toBe("api_error_internal");
  });

  it("puts the reserved value in the sentence", () => {
    expect(apiErrorMessage(body("value_reserved", { value: "admin" }))).toBe(
      'api_error_value_reserved {"value":"admin"}'
    );
  });

  it("a password that is too short names the minimum, and the field name is translated", () => {
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

  it("a value that is too long names the maximum", () => {
    expect(
      apiErrorMessage(
        body("value_length", { actual: 40, field: "username", max: 32, min: 3 })
      )
    ).toBe(
      'api_error_value_too_long {"field":"api_error_field_username","max":32}'
    );
  });

  it("a length error with only max (share title, file name) is always too long", () => {
    expect(
      apiErrorMessage(body("value_length", { field: "title", max: 200 }))
    ).toBe(
      'api_error_value_too_long {"field":"api_error_field_title","max":200}'
    );
  });

  it("a format error gets the sentence for its rule", () => {
    expect(
      apiErrorMessage(
        body("value_invalid", { field: "password", rule: "needs_digit" })
      )
    ).toBe("api_error_rule_needs_digit");
  });

  it('an unknown rule falls back to "not in the right format"', () => {
    expect(
      apiErrorMessage(
        body("value_invalid", { field: "email", rule: "brand_new" })
      )
    ).toBe('api_error_value_invalid {"field":"api_error_field_email"}');
  });

  it("an unknown field does not put the API's field name in the sentence", () => {
    expect(apiErrorMessage(body("value_empty", { field: "key_salt" }))).toBe(
      'api_error_value_empty {"field":"api_error_field_unknown"}'
    );
  });

  it("a range error picks its sentence by which bounds it has", () => {
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

  it("waits under a minute are in seconds, longer ones in minutes (rounded up)", () => {
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

  it("a byte limit is formatted as a readable size", () => {
    expect(
      apiErrorMessage(body("payload_too_large", { limit_bytes: 1_048_576 }))
    ).toBe('api_error_payload_too_large_limit {"limit":"1.0 MB"}');
  });

  it("storage errors are worded by tier", () => {
    expect(
      apiErrorMessage(body("insufficient_storage", { tier: "role" }))
    ).toBe("api_error_insufficient_storage_role");
  });

  it("a code whose reason is only in message gets the message after the translation", () => {
    expect(
      apiErrorMessage({
        code: "provider_unreachable",
        details: {},
        message: "connection refused",
      })
    ).toBe('api_error_provider_unreachable {"reason":"connection refused"}');
  });

  it("a code the frontend does not know yet falls back to the server's message", () => {
    expect(apiErrorMessage(body("added_next_release"))).toBe(
      "server says added_next_release"
    );
  });

  it("no code (a dropped connection, an Error from the frontend) uses the fallback, not the internal string", () => {
    expect(apiErrorMessage(new TypeError("Failed to fetch"), "try again")).toBe(
      "try again"
    );
    expect(apiErrorMessage(null)).toBe("api_error_unknown");
  });

  it("an upload that got no answer says so; a proxy's bodyless refusal is read from its status", () => {
    expect(
      apiErrorMessage(
        Object.assign(new Error("x"), { network: true }),
        "try again"
      )
    ).toBe("api_error_network");
    expect(
      apiErrorMessage(
        Object.assign(new Error("x"), { status: 413 }),
        "try again"
      )
    ).toBe("api_error_payload_too_large");
    expect(
      apiErrorMessage(
        Object.assign(new Error("x"), { status: 502 }),
        "try again"
      )
    ).toBe("api_error_unavailable");
    expect(
      apiErrorMessage(
        Object.assign(new Error("x"), { status: 400 }),
        "try again"
      )
    ).toBe("try again");
  });

  it("an instance that refuses anonymous uploads says that, not 'sign in again'", () => {
    expect(apiErrorMessage(body("anonymous_upload_disabled"))).toBe(
      "api_error_anonymous_upload_disabled"
    );
  });

  it("missing or mistyped details do not throw", () => {
    expect(
      apiErrorMessage({ code: "value_length", details: null, message: "x" })
    ).toBe('api_error_value_length {"field":"api_error_field_unknown"}');
    expect(
      apiErrorMessage(body("value_length", { field: "password", min: "12" }))
    ).toBe('api_error_value_length {"field":"api_error_field_password"}');
  });

  it("an Error thrown during upload with a code attached is translated too", () => {
    const error = Object.assign(new Error("that upload was abandoned"), {
      code: "upload_abandoned",
      details: {},
    });
    expect(apiErrorMessage(error)).toBe("api_error_upload_abandoned");
  });
});

describe("apiErrorCode", () => {
  it("reads the code, or undefined when there is no object or no code", () => {
    expect(apiErrorCode(body("password_breached"))).toBe("password_breached");
    expect(apiErrorCode(new Error("nope"))).toBeUndefined();
    expect(apiErrorCode("value_empty")).toBeUndefined();
  });
});
