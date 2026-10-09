import { describe, expect, it, vi } from "vite-plus/test";

import { runResumableUpload } from "./resumable";
import type {
  PatchOutcome,
  ResumableIo,
  UploadSessionResponse,
} from "./resumable";
import type { ShareResponse } from "./share-api";

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
