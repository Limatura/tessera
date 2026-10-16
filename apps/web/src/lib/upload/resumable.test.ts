import { describe, expect, it, vi } from "vite-plus/test";

import { runResumableUpload, streamSource } from "./resumable";
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
 * Parts are PUT straight to the store, bypassing the server. No real XHR or
 * fetch here: the I/O is faked to test only `runResumableUpload`'s decisions,
 * namely the direct order, falling back to PATCH when a PUT fails, never
 * switching back, and the order of the etags array.
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

  it("with direct: true every part is a PUT, ETags are collected in order, then complete is called", async () => {
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

  it("when one part's PUT fails it switches to PATCH, and later parts no longer try direct", async () => {
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

    // Part 1 goes direct, part 2 fails direct and uses PATCH, part 3 uses PATCH without
    // trying direct
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
    // complete carries only the ETag of the one part that went direct
    expect(completeUpload).toHaveBeenCalledWith(
      "session-1",
      ['"etag-1"'],
      "upload-token",
      undefined
    );
  });

  it("a direct: false session never calls putPartDirect", async () => {
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

  it("when the last PATCH answers 201 it returns the share without calling complete", async () => {
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

  it("a session without a token falls back to the managementToken the caller passed", async () => {
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

  it("onProgress reports progress by bytes sent and ends at 100", async () => {
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

function streamOf(pieces: string[]) {
  return new ReadableStream<Uint8Array>({
    start: (controller) => {
      for (const piece of pieces) {
        controller.enqueue(new TextEncoder().encode(piece));
      }
      controller.close();
    },
  });
}

describe("streamSource", () => {
  it("cuts parts across the stream's own piece boundaries", async () => {
    const source = streamSource(streamOf(["abc", "defgh", "ij"]), 10);
    const parts = [
      await source.slice(0, 4),
      await source.slice(4, 8),
      await source.slice(8, 10),
    ];
    const texts = await Promise.all(parts.map((part) => part.text()));
    expect(texts).toEqual(["abcd", "efgh", "ij"]);
  });

  it("refuses a part asked for out of order", async () => {
    const source = streamSource(streamOf(["abcdef"]), 6);
    await expect(source.slice(3, 6)).rejects.toThrow("out of order");
  });

  it("refuses a stream shorter than it said", async () => {
    const source = streamSource(streamOf(["abc"]), 6);
    await expect(source.slice(0, 6)).rejects.toThrow("ended");
  });
});
