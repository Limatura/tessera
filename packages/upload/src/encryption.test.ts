import {
  CHUNK_LENGTH,
  HEADER_LENGTH,
  keyFromPassword,
  newKey,
  openBytes,
  openName,
  openStream,
  openText,
  sealedSize,
  ShareKey,
} from "@tessera/crypto";
import { afterEach, describe, expect, it, vi } from "vite-plus/test";

import {
  prepareSealing,
  sealedBlob,
  sealedDetailsFor,
  sealedNameFor,
  sealedSource,
  sealedText,
} from "./encryption";
import { byteDiff } from "./testing/bytes";

/** What the crate says for an altered object or the wrong key: one message for both. */
const UNAUTHENTIC = /altered, or the key is wrong/u;

afterEach(() => {
  vi.unstubAllGlobals();
});

const randomBytes = (length: number) => {
  const out = new Uint8Array(length);
  // getRandomValues takes at most 65536 bytes at a time
  for (let offset = 0; offset < length; offset += 65_536) {
    crypto.getRandomValues(out.subarray(offset, offset + 65_536));
  }
  return out;
};

const bytesOf = async (blob: Blob) => new Uint8Array(await blob.arrayBuffer());

const fileOf = (bytes: Uint8Array, name = "a.bin") =>
  new File([bytes as BlobPart], name, { type: "application/octet-stream" });

const open = async (
  key: ReturnType<typeof newKey>,
  sealed: Uint8Array
): Promise<Uint8Array> => {
  const source = new ReadableStream<Uint8Array>({
    start(controller) {
      controller.enqueue(sealed);
      controller.close();
    },
  });
  return new Uint8Array(
    await new Response(
      source.pipeThrough(openStream(key, sealed.length))
    ).arrayBuffer()
  );
};

/** Reads a source the way a resumable upload does: part by part, in order. */
const readParts = async (
  source: { size: number; slice: (s: number, e: number) => Promise<Blob> },
  partSize: number
) => {
  const parts: Blob[] = [];
  for (let start = 0; start < source.size; start += partSize) {
    const end = Math.min(start + partSize, source.size);
    // oxlint-disable-next-line no-await-in-loop -- parts are read in order
    parts.push(await source.slice(start, end));
  }
  return new Uint8Array(await new Blob(parts).arrayBuffer());
};

const sizes: [string, number][] = [
  ["empty", 0],
  ["one byte", 1],
  ["exactly one chunk", CHUNK_LENGTH],
  ["exactly three chunks", CHUNK_LENGTH * 3],
  ["three chunks and one byte", CHUNK_LENGTH * 3 + 1],
  ["about a megabyte", 1_000_003],
];

describe("sealedSource", () => {
  it.each(sizes)("round-trips %s through upload parts", async (_, size) => {
    const key = newKey();
    const plain = randomBytes(size);
    const source = sealedSource(key, fileOf(plain));
    expect(source.size).toBe(sealedSize(size));

    const sealed = await readParts(source, 100_000);
    expect(sealed.length).toBe(source.size);
    expect(byteDiff(await open(key, sealed), plain)).toBeNull();
  });

  it("refuses parts asked for out of order", async () => {
    const source = sealedSource(newKey(), fileOf(randomBytes(10)));
    await expect(source.slice(5, 10)).rejects.toThrow(/out of order/u);
  });
});

describe("sealedBlob", () => {
  it.each(sizes)("round-trips %s", async (_, size) => {
    const key = newKey();
    const plain = randomBytes(size);
    const blob = await sealedBlob(key, fileOf(plain));
    expect(blob.size).toBe(sealedSize(size));
    expect(byteDiff(await open(key, await bytesOf(blob)), plain)).toBeNull();
  });

  it("never seals the same file to the same bytes", async () => {
    const key = newKey();
    const file = fileOf(randomBytes(100));
    const one = await bytesOf(await sealedBlob(key, file));
    const two = await bytesOf(await sealedBlob(key, file));
    expect(one.subarray(0, HEADER_LENGTH)).not.toEqual(
      two.subarray(0, HEADER_LENGTH)
    );
    expect(Buffer.compare(one, two)).not.toBe(0);
  });

  it("seals identical chunks of one file to different bytes", async () => {
    const key = newKey();
    const sealed = await bytesOf(
      await sealedBlob(key, fileOf(new Uint8Array(CHUNK_LENGTH * 3)))
    );
    const stride = sealedSize(CHUNK_LENGTH) - HEADER_LENGTH;
    const chunk = (index: number) =>
      sealed.subarray(
        HEADER_LENGTH + index * stride,
        HEADER_LENGTH + (index + 1) * stride
      );
    expect(chunk(0)).not.toEqual(chunk(1));
    expect(chunk(1)).not.toEqual(chunk(2));
  });
});

describe("sealedText", () => {
  it("opens back to the same text, including the empty string", async () => {
    const key = newKey();
    for (const text of ["", "x", "筆記 📝\n".repeat(1000)]) {
      // oxlint-disable-next-line no-await-in-loop -- small, sequential
      const sealed = new Uint8Array(await sealedText(key, text).arrayBuffer());
      expect(new TextDecoder().decode(openBytes(key, sealed))).toBe(text);
    }
  });

  it("does not open with another key", async () => {
    const sealed = new Uint8Array(
      await sealedText(newKey(), "secret").arrayBuffer()
    );
    expect(() => openBytes(newKey(), sealed)).toThrow(UNAUTHENTIC);
  });

  it("does not open once a byte is changed", async () => {
    const key = newKey();
    const sealed = new Uint8Array(
      await sealedText(key, "secret").arrayBuffer()
    );
    sealed[sealed.length - 1] = ((sealed.at(-1) ?? 0) + 1) % 256;
    expect(() => openBytes(key, sealed)).toThrow(UNAUTHENTIC);
  });
});

describe("sealedNameFor and sealedDetailsFor", () => {
  it("open back to the path, type and details", () => {
    const key = newKey();
    const file = fileOf(new Uint8Array(1), "b.txt");
    expect(openName(key, sealedNameFor(key, file, "docs/b.txt"))).toEqual({
      name: "docs/b.txt",
      type: "application/octet-stream",
    });
    const details = { language: "rust", title: "標題" };
    expect(JSON.parse(openText(key, sealedDetailsFor(key, details)))).toEqual(
      details
    );
  });

  it("leave the type out when the file has none", () => {
    const key = newKey();
    const file = new File(["x"], "c");
    expect(openName(key, sealedNameFor(key, file, "c"))).toEqual({ name: "c" });
  });
});

describe("prepareSealing", () => {
  it("puts a link key in the fragment that decodes to the same key", async () => {
    const sealing = await prepareSealing({ mode: "link" });
    expect(sealing.query).toEqual({ encryption: "link" });
    expect(sealing.accessToken).toBeUndefined();
    const sealed = sealedNameFor(sealing.key, new File(["x"], "n"), "n");
    const decoded = ShareKey.parse(sealing.fragment ?? "");
    expect(openName(decoded, sealed)).toEqual({ name: "n" });
  });

  it("derives a password key from the salt it sends, and no fragment", async () => {
    vi.stubGlobal("requestAnimationFrame", (frame: () => void) => {
      frame();
      return 0;
    });
    const password = "correct horse battery";
    const sealing = await prepareSealing({ mode: "password", password });
    expect(sealing.fragment).toBeUndefined();
    expect(sealing.query.encryption).toBe("password");
    expect(sealing.accessToken).toBeTruthy();

    const again = keyFromPassword(password, sealing.query.key_salt ?? "");
    expect(again.accessToken()).toBe(sealing.accessToken);
    const sealed = sealedNameFor(sealing.key, new File(["x"], "n"), "n");
    expect(openName(again, sealed)).toEqual({ name: "n" });
  });
});
