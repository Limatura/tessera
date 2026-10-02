import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vite-plus/test";

import type {
  PatchOutcome,
  ResumableIo,
  ShareResponse,
  UploadSessionResponse,
} from "./upload";
import { runResumableUpload, updateShare } from "./upload";

/**
 * `PATCH /shares/{code}` 分辨「沒提到這個欄位」和「明確送 null」:前者不動、
 * 後者清除(後端是 `Patch<T> = Option<Option<T>>`)。而且 body 有
 * `deny_unknown_fields`,多送一個 key 整個請求會被拒。
 *
 * 密碼是特例:設定走 `X-Share-Password` header,移除走 body 的
 * `remove_password`,後端明講兩個一起送會被拒絕。
 */
function lastRequest() {
  const mock = vi.mocked(globalThis.fetch);
  const [url, init] = mock.mock.calls.at(-1) as [string, RequestInit];
  return {
    body: JSON.parse(init.body as string) as Record<string, unknown>,
    headers: init.headers as Record<string, string>,
    method: init.method,
    url,
  };
}

describe("updateShare", () => {
  beforeEach(() => {
    globalThis.fetch = vi.fn<typeof fetch>(() =>
      Promise.resolve(new Response(null, { status: 200 }))
    );
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("只送出呼叫端真的指定的欄位", async () => {
    await updateShare("abc", { maxUses: 1 });
    const { body, method, url } = lastRequest();
    expect(method).toBe("PATCH");
    expect(url).toBe("/api/v1/shares/abc");
    expect(body).toEqual({ max_uses: 1 });
  });

  it("過期時間 0 或 undefined 都送 null(代表不再過期)", async () => {
    await updateShare("abc", { expiresInHours: 0 });
    expect(lastRequest().body).toEqual({ expires_in_hours: null });

    await updateShare("abc", { expiresInHours: undefined });
    expect(lastRequest().body).toEqual({ expires_in_hours: null });
  });

  it("設定密碼走 header,body 不帶 remove_password", async () => {
    await updateShare("abc", { password: "s3cret" });
    const { body, headers } = lastRequest();
    expect(headers["X-Share-Password"]).toBe("s3cret");
    expect(body).not.toHaveProperty("remove_password");
  });

  it("移除密碼走 body,而且不送密碼 header(後端會拒絕兩個一起送)", async () => {
    await updateShare("abc", { password: undefined });
    const { body, headers } = lastRequest();
    expect(body).toEqual({ remove_password: true });
    expect(headers).not.toHaveProperty("X-Share-Password");
  });

  it("body 只會出現後端認得的 key", async () => {
    await updateShare("abc", {
      expiresInHours: 24,
      maxUses: 2,
      password: undefined,
    });
    expect(Object.keys(lastRequest().body).toSorted()).toEqual([
      "expires_in_hours",
      "max_uses",
      "remove_password",
    ]);
  });

  it("有 management token 就帶上", async () => {
    await updateShare("abc", { maxUses: 1 }, "tok-123");
    expect(lastRequest().headers["X-Share-Management-Token"]).toBe("tok-123");
  });

  it("失敗時丟出後端給的訊息", async () => {
    globalThis.fetch = vi.fn<typeof fetch>(() =>
      Promise.resolve(Response.json({ message: "Nope" }, { status: 400 }))
    );
    await expect(updateShare("abc", { maxUses: 1 })).rejects.toThrow("Nope");
  });
});

function finalShare(): ShareResponse {
  return { code: "ABC123" } as ShareResponse;
}

/**
 * issue #283:分片直接 PUT 到 store,不經過伺服器。這裡不打真的 XHR/fetch,
 * 而是把 I/O 換成假的,只驗證 `runResumableUpload` 自己的決策邏輯:
 * 走 direct 的順序、PUT 失敗要退回 PATCH、之後不再切回去、etags 陣列的順序。
 */
describe("runResumableUpload", () => {
  const PART_SIZE = 10;

  function session(overrides: Partial<UploadSessionResponse> = {}) {
    return {
      direct: true,
      id: "session-1",
      length: 0,
      offset: 0,
      part_size: PART_SIZE,
      token: "upload-token",
      ...overrides,
    } satisfies UploadSessionResponse;
  }

  it("direct: true 時每片都走 PUT,依序收集 ETag,最後呼叫 complete", async () => {
    const file = new File(["a".repeat(25)], "big.bin");
    const putPartDirect = vi
      .fn<ResumableIo["putPartDirect"]>()
      .mockImplementation((_id, partNumber) =>
        Promise.resolve(`"etag-${partNumber}"`)
      );
    const patchPart = vi.fn<ResumableIo["patchPart"]>();
    const completeUpload = vi
      .fn<ResumableIo["completeUpload"]>()
      .mockResolvedValue(finalShare());

    const result = await runResumableUpload(file, session(), {}, undefined, {
      completeUpload,
      patchPart,
      putPartDirect,
    } satisfies ResumableIo);

    expect(putPartDirect).toHaveBeenCalledTimes(3);
    const [firstCallArgs] = putPartDirect.mock.calls;
    expect(firstCallArgs?.slice(0, 4)).toEqual([
      "session-1",
      1,
      expect.any(Blob),
      "upload-token",
    ]);
    expect(patchPart).not.toHaveBeenCalled();
    expect(completeUpload).toHaveBeenCalledWith(
      "session-1",
      ['"etag-1"', '"etag-2"', '"etag-3"'],
      "upload-token",
      undefined
    );
    expect(result).toEqual(finalShare());
  });

  it("某一片 PUT 失敗就改走 PATCH,之後的片也不再嘗試 direct", async () => {
    const file = new File(["a".repeat(25)], "big.bin");
    const putPartDirect = vi
      .fn<ResumableIo["putPartDirect"]>()
      .mockImplementation((_id, partNumber) =>
        partNumber === 1
          ? Promise.resolve('"etag-1"')
          : Promise.reject(new Error("bucket has no CORS"))
      );
    const patchPart = vi
      .fn<ResumableIo["patchPart"]>()
      .mockResolvedValue({ done: false } satisfies PatchOutcome);
    const completeUpload = vi
      .fn<ResumableIo["completeUpload"]>()
      .mockResolvedValue(finalShare());

    await runResumableUpload(file, session(), {}, undefined, {
      completeUpload,
      patchPart,
      putPartDirect,
    } satisfies ResumableIo);

    // 第 1 片直傳成功,第 2 片直傳失敗後轉 PATCH,第 3 片直接走 PATCH,不再試 direct
    expect(putPartDirect).toHaveBeenCalledTimes(2);
    expect(patchPart).toHaveBeenCalledTimes(2);
    expect(patchPart).toHaveBeenNthCalledWith(
      1,
      "session-1",
      10,
      expect.any(Blob),
      "upload-token",
      expect.objectContaining({ onProgress: expect.any(Function) })
    );
    expect(patchPart).toHaveBeenNthCalledWith(
      2,
      "session-1",
      20,
      expect.any(Blob),
      "upload-token",
      expect.objectContaining({ onProgress: expect.any(Function) })
    );
    // complete 只帶第一片真的直傳成功的 ETag
    expect(completeUpload).toHaveBeenCalledWith(
      "session-1",
      ['"etag-1"'],
      "upload-token",
      undefined
    );
  });

  it("direct: false 的 session 完全不呼叫 putPartDirect", async () => {
    const file = new File(["a".repeat(15)], "small.bin");
    const putPartDirect = vi.fn<ResumableIo["putPartDirect"]>();
    const patchPart = vi
      .fn<ResumableIo["patchPart"]>()
      .mockResolvedValue({ done: false } satisfies PatchOutcome);
    const completeUpload = vi
      .fn<ResumableIo["completeUpload"]>()
      .mockResolvedValue(finalShare());

    await runResumableUpload(file, session({ direct: false }), {}, undefined, {
      completeUpload,
      patchPart,
      putPartDirect,
    } satisfies ResumableIo);

    expect(putPartDirect).not.toHaveBeenCalled();
    expect(patchPart).toHaveBeenCalledTimes(2);
  });

  it("PATCH 最後一片回 201 時直接回傳 share,不再呼叫 complete", async () => {
    const file = new File(["a".repeat(15)], "small.bin");
    const patchPart = vi
      .fn<ResumableIo["patchPart"]>()
      .mockResolvedValueOnce({ done: false } satisfies PatchOutcome)
      .mockResolvedValueOnce({
        done: true,
        share: finalShare(),
      } satisfies PatchOutcome);
    const completeUpload = vi.fn<ResumableIo["completeUpload"]>();

    const result = await runResumableUpload(
      file,
      session({ direct: false }),
      {},
      undefined,
      {
        completeUpload,
        patchPart,
        putPartDirect: vi.fn<ResumableIo["putPartDirect"]>(),
      } satisfies ResumableIo
    );

    expect(result).toEqual(finalShare());
    expect(completeUpload).not.toHaveBeenCalled();
  });

  it("session 沒帶 token 時退回用呼叫端傳入的 managementToken", async () => {
    const file = new File(["a".repeat(5)], "tiny.bin");
    const putPartDirect = vi
      .fn<ResumableIo["putPartDirect"]>()
      .mockResolvedValue('"etag-1"');
    const completeUpload = vi
      .fn<ResumableIo["completeUpload"]>()
      .mockResolvedValue(finalShare());

    await runResumableUpload(
      file,
      session({ token: null }),
      {},
      "management-token-from-caller",
      {
        completeUpload,
        patchPart: vi.fn<ResumableIo["patchPart"]>(),
        putPartDirect,
      } satisfies ResumableIo
    );

    expect(putPartDirect).toHaveBeenCalledWith(
      "session-1",
      1,
      expect.any(Blob),
      "management-token-from-caller",
      undefined
    );
  });

  it("onProgress 依實際位元組進度回報,終值為 100", async () => {
    const file = new File(["a".repeat(25)], "big.bin");
    const putPartDirect = vi
      .fn<ResumableIo["putPartDirect"]>()
      .mockResolvedValue('"etag"');
    const completeUpload = vi
      .fn<ResumableIo["completeUpload"]>()
      .mockResolvedValue(finalShare());
    const progress: number[] = [];

    await runResumableUpload(
      file,
      session(),
      { onProgress: (percent) => progress.push(percent) },
      undefined,
      {
        completeUpload,
        patchPart: vi.fn<ResumableIo["patchPart"]>(),
        putPartDirect,
      } satisfies ResumableIo
    );

    expect(progress.at(-1)).toBe(100);
    expect(progress).toEqual([...progress].toSorted((a, b) => a - b));
  });
});
