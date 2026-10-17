import type { components } from "@tessera/api/schema";
import {
  CHUNK_LENGTH,
  newKey,
  sealDetails,
  sealName,
  sealStream,
} from "@tessera/crypto";
import type { ShareKey } from "@tessera/crypto";
import { afterEach, describe, expect, it, vi } from "vite-plus/test";

import { byteDiff } from "../../tests/unit/bytes";
import type { OpenedFile } from "./encrypted";
import {
  baseName,
  openShare,
  openedFileBlob,
  openedFileStream,
  safePath,
} from "./encrypted";

type PublicShare = components["schemas"]["PublicShareResponse"];

/** What the crate says for an altered object or the wrong key: one message for both. */
const UNAUTHENTIC = /altered, or the key is wrong/u;

afterEach(() => {
  vi.unstubAllGlobals();
});

const randomBytes = (length: number) => {
  const out = new Uint8Array(length);
  for (let offset = 0; offset < length; offset += 65_536) {
    crypto.getRandomValues(out.subarray(offset, offset + 65_536));
  }
  return out;
};

const collect = async (stream: ReadableStream<Uint8Array>) =>
  new Uint8Array(await new Response(stream).arrayBuffer());

const seal = (key: ShareKey, plain: Uint8Array) =>
  collect(new Blob([plain as BlobPart]).stream().pipeThrough(sealStream(key)));

const shareOf = (files: Partial<PublicShare["files"][number]>[], extra = {}) =>
  ({ files, ...extra }) as unknown as PublicShare;

describe("openShare", () => {
  it("opens the title, names and sizes with the right key", async () => {
    const key = newKey();
    const sealed = await seal(key, randomBytes(CHUNK_LENGTH + 5));
    const opened = openShare(
      key,
      shareOf(
        [
          {
            sealed_name: sealName(key, { name: "a/b.txt", type: "text/plain" }),
            size_bytes: sealed.length,
          },
        ],
        { sealed_details: sealDetails(key, { language: "go", title: "T" }) }
      )
    );
    expect(opened).toEqual({
      files: [
        {
          index: 0,
          path: "a/b.txt",
          plainBytes: CHUNK_LENGTH + 5,
          sealedBytes: sealed.length,
          type: "text/plain",
        },
      ],
      language: "go",
      title: "T",
    });
  });

  it("throws instead of returning names for the wrong key", () => {
    const key = newKey();
    const share = shareOf([
      { sealed_name: sealName(key, { name: "x" }), size_bytes: 17 + 16 },
    ]);
    expect(() => openShare(newKey(), share)).toThrow(UNAUTHENTIC);
  });

  it("throws for a tampered name", () => {
    const key = newKey();
    const name = sealName(key, { name: "x" });
    const flipped = `${name.slice(0, -2)}${name.at(-2) === "A" ? "B" : "A"}${name.at(-1)}`;
    expect(() =>
      openShare(key, shareOf([{ sealed_name: flipped, size_bytes: 33 }]))
    ).toThrow(UNAUTHENTIC);
  });

  it("keeps a sealed path from climbing out of the folder", () => {
    const key = newKey();
    const opened = openShare(
      key,
      shareOf([
        {
          sealed_name: sealName(key, { name: "../../etc/passwd" }),
          size_bytes: 33,
        },
        { sealed_name: sealName(key, { name: "/.." }), size_bytes: 33 },
        { size_bytes: 33 },
      ])
    );
    expect(opened.files.map((file) => file.path)).toEqual([
      "etc/passwd",
      "file-2",
      "file-3",
    ]);
  });
});

describe("safePath and baseName", () => {
  it("drop empty, current and parent parts on either slash", () => {
    expect(safePath(String.raw`a\..\b/./c//d`)).toBe("a/b/c/d");
    expect(safePath("..")).toBe("");
    expect(baseName("a/b/c.txt")).toBe("c.txt");
    expect(baseName("c.txt")).toBe("c.txt");
  });
});

const fileFor = (sealed: Uint8Array, plain: number): OpenedFile => ({
  index: 1,
  path: "f",
  plainBytes: plain,
  sealedBytes: sealed.length,
});

const layoutOf2 = (length: number) => {
  const cut = Math.floor(length / 2);
  return {
    segments: [
      { length: cut, offset: 0 },
      { length: length - cut, offset: cut },
    ],
    size_bytes: length,
  };
};

describe("openedFileStream", () => {
  it("joins the pieces from the stores and opens them", async () => {
    const key = newKey();
    const plain = randomBytes(CHUNK_LENGTH * 4 + 9);
    const sealed = await seal(key, plain);
    const cut = Math.floor(sealed.length / 2);
    const layout = {
      segments: [
        { length: cut, offset: 0 },
        { length: sealed.length - cut, offset: cut },
      ],
      size_bytes: sealed.length,
    };
    const fetchMock = vi.fn<(url: string) => Promise<Response>>(
      (url: string) => {
        if (url === "/s/abc/segments?file=1") {
          return Promise.resolve(Response.json(layout));
        }
        const index = Number(
          /segments\/(?<index>\d)/u.exec(url)?.groups?.index
        );
        const segment = layout.segments[index];
        return Promise.resolve(
          new Response(
            sealed.subarray(
              segment?.offset,
              (segment?.offset ?? 0) + (segment?.length ?? 0)
            )
          )
        );
      }
    );
    vi.stubGlobal("fetch", fetchMock);
    const ticket = vi.fn<() => Promise<string>>(() => Promise.resolve("t"));

    const stream = await openedFileStream(
      "abc",
      key,
      fileFor(sealed, plain.length),
      ticket
    );
    expect(byteDiff(await collect(stream), plain)).toBeNull();
    expect(ticket).not.toHaveBeenCalled();
    expect(fetchMock.mock.calls.map(([url]) => url)).toEqual([
      "/s/abc/segments?file=1",
      "/s/abc/segments/0?file=1",
      "/s/abc/segments/1?file=1",
    ]);
  });

  it("falls back to /download when the first segment fetch throws", async () => {
    const key = newKey();
    const plain = randomBytes(CHUNK_LENGTH * 2 + 3);
    const sealed = await seal(key, plain);
    const layout = layoutOf2(sealed.length);
    const fetchMock = vi.fn<(url: string) => Promise<Response>>(
      (url: string) => {
        if (url === "/s/abc/segments?file=1") {
          return Promise.resolve(Response.json(layout));
        }
        if (url.startsWith("/s/abc/segments/")) {
          // What a bucket without CORS looks like to fetch
          return Promise.reject(new TypeError("Failed to fetch"));
        }
        return Promise.resolve(new Response(sealed as BodyInit));
      }
    );
    vi.stubGlobal("fetch", fetchMock);
    const ticket = vi.fn<() => Promise<string>>(() => Promise.resolve("tk"));

    const stream = await openedFileStream(
      "abc",
      key,
      fileFor(sealed, plain.length),
      ticket
    );
    expect(byteDiff(await collect(stream), plain)).toBeNull();
    expect(ticket).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls.map(([url]) => url)).toEqual([
      "/s/abc/segments?file=1",
      "/s/abc/segments/0?file=1",
      "/s/abc/download?t=tk&file=1",
    ]);
  });

  it("uses /download for a file stored as a single segment", async () => {
    const key = newKey();
    const plain = randomBytes(5000);
    const sealed = await seal(key, plain);
    const layout = {
      segments: [{ length: sealed.length, offset: 0 }],
      size_bytes: sealed.length,
    };
    const fetchMock = vi.fn<(url: string) => Promise<Response>>((url: string) =>
      Promise.resolve(
        url === "/s/abc/segments?file=1"
          ? Response.json(layout)
          : new Response(sealed as BodyInit)
      )
    );
    vi.stubGlobal("fetch", fetchMock);
    const ticket = vi.fn<() => Promise<string>>(() => Promise.resolve("tk"));

    const stream = await openedFileStream(
      "abc",
      key,
      fileFor(sealed, plain.length),
      ticket
    );
    expect(byteDiff(await collect(stream), plain)).toBeNull();
    expect(fetchMock.mock.calls.map(([url]) => url)).toEqual([
      "/s/abc/segments?file=1",
      "/s/abc/download?t=tk&file=1",
    ]);
  });

  it("falls back to /download, spending a ticket, when there is no layout", async () => {
    const key = newKey();
    const plain = randomBytes(1000);
    const sealed = await seal(key, plain);
    const fetchMock = vi.fn<(url: string) => Promise<Response>>((url: string) =>
      Promise.resolve(
        url.includes("/segments")
          ? new Response(null, { status: 404 })
          : new Response(sealed)
      )
    );
    vi.stubGlobal("fetch", fetchMock);

    const blob = await openedFileBlob(
      "abc",
      key,
      { ...fileFor(sealed, 1000), type: "text/plain" },
      () => Promise.resolve("tk")
    );
    expect(
      byteDiff(new Uint8Array(await blob.arrayBuffer()), plain)
    ).toBeNull();
    expect(blob.type).toBe("text/plain");
    expect(fetchMock).toHaveBeenLastCalledWith("/s/abc/download?t=tk&file=1");
  });

  it("rejects /download bytes opened with the wrong key", async () => {
    const sealed = await seal(newKey(), randomBytes(CHUNK_LENGTH * 2));
    vi.stubGlobal(
      "fetch",
      vi.fn<(url: string) => Promise<Response>>((url: string) =>
        Promise.resolve(
          url.includes("/segments")
            ? new Response(null, { status: 404 })
            : new Response(sealed)
        )
      )
    );
    const stream = await openedFileStream(
      "abc",
      newKey(),
      fileFor(sealed, CHUNK_LENGTH * 2),
      () => Promise.resolve("t")
    );
    await expect(collect(stream)).rejects.toThrow(UNAUTHENTIC);
  });

  it("rejects a /download that ends early", async () => {
    const key = newKey();
    const sealed = await seal(key, randomBytes(CHUNK_LENGTH * 2));
    vi.stubGlobal(
      "fetch",
      vi.fn<(url: string) => Promise<Response>>((url: string) =>
        Promise.resolve(
          url.includes("/segments")
            ? new Response(null, { status: 404 })
            : new Response(sealed.subarray(0, -10))
        )
      )
    );
    const stream = await openedFileStream(
      "abc",
      key,
      fileFor(sealed, CHUNK_LENGTH * 2),
      () => Promise.resolve("t")
    );
    await expect(collect(stream)).rejects.toThrow(/ended early/u);
  });
});
