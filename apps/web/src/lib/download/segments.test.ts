import { CHUNK_LENGTH, newKey, openStream, sealStream } from "@tessera/crypto";
import type { ShareKey } from "@tessera/crypto";
import { afterEach, describe, expect, it, vi } from "vite-plus/test";

import { byteDiff } from "../../tests/unit/bytes";
import type { SegmentLayout } from "./segments";
import { fetchSegment, fetchSegmentLayout, joinSegments } from "./segments";

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

const streamOf = (...pieces: Uint8Array[]) =>
  new ReadableStream<Uint8Array>({
    start(controller) {
      for (const piece of pieces) {
        controller.enqueue(piece);
      }
      controller.close();
    },
  });

const collect = async (stream: ReadableStream<Uint8Array>) =>
  new Uint8Array(await new Response(stream).arrayBuffer());

const seal = (key: ShareKey, plain: Uint8Array) =>
  collect(streamOf(plain).pipeThrough(sealStream(key)));

/** Cuts `bytes` into `count` pieces as even as possible, as a store split would. */
const layoutOf = (length: number, count: number): SegmentLayout => {
  const segments: SegmentLayout["segments"] = [];
  const base = Math.floor(length / count);
  let offset = 0;
  for (let index = 0; index < count; index += 1) {
    const size = index === count - 1 ? length - offset : base;
    segments.push({ length: size, offset });
    offset += size;
  }
  return { segments, size_bytes: length };
};

/**
 * Answers `/segments/{i}` with whatever `body(i)` says, in small reads so a
 * piece arrives in more than one chunk.
 */
const stubStore = (body: (index: number) => Uint8Array) => {
  const fetchMock = vi.fn<(input: string) => Promise<Response>>(
    (input: string) => {
      const match = /\/segments\/(?<index>\d+)/u.exec(input);
      const index = Number(match?.groups?.index);
      const bytes = body(index);
      const reads: Uint8Array[] = [];
      for (let at = 0; at < bytes.length; at += 40_000) {
        reads.push(bytes.subarray(at, at + 40_000));
      }
      return Promise.resolve(new Response(streamOf(...reads)));
    }
  );
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
};

const pieceOf = (sealed: Uint8Array, layout: SegmentLayout, index: number) => {
  const segment = layout.segments[index];
  return segment
    ? sealed.subarray(segment.offset, segment.offset + segment.length)
    : new Uint8Array();
};

const joinAndOpen = async (
  key: ShareKey,
  layout: SegmentLayout,
  sealedLength: number,
  file?: number
) => {
  const first = await fetchSegment("abc", 0, file);
  return collect(
    joinSegments("abc", layout, first, file).pipeThrough(
      openStream(key, sealedLength)
    )
  );
};

const sizes: [string, number, number][] = [
  ["empty", 0, 1],
  ["one byte", 1, 1],
  ["exactly one chunk", CHUNK_LENGTH, 2],
  ["exactly three chunks", CHUNK_LENGTH * 3, 3],
  ["three chunks and one byte", CHUNK_LENGTH * 3 + 1, 3],
  ["about a megabyte", 1_000_003, 5],
];

describe("seal, split into segments, join, open", () => {
  it.each(sizes)("restores %s across the pieces", async (_, size, count) => {
    const key = newKey();
    const plain = randomBytes(size);
    const sealed = await seal(key, plain);
    const layout = layoutOf(sealed.length, count);
    stubStore((index) => pieceOf(sealed, layout, index));

    expect(
      byteDiff(await joinAndOpen(key, layout, sealed.length), plain)
    ).toBeNull();
  });

  it("restores a file split on the exact chunk boundaries", async () => {
    const key = newKey();
    const plain = randomBytes(CHUNK_LENGTH * 3);
    const sealed = await seal(key, plain);
    const stride = (sealed.length - 17) / 3;
    const layout: SegmentLayout = {
      segments: [
        { length: 17 + stride, offset: 0 },
        { length: stride, offset: 17 + stride },
        { length: stride, offset: 17 + stride * 2 },
      ],
      size_bytes: sealed.length,
    };
    stubStore((index) => pieceOf(sealed, layout, index));
    expect(
      byteDiff(await joinAndOpen(key, layout, sealed.length), plain)
    ).toBeNull();
  });

  it("fetches each later piece only once, in order, for the file asked", async () => {
    const key = newKey();
    const sealed = await seal(key, randomBytes(CHUNK_LENGTH * 2));
    const layout = layoutOf(sealed.length, 4);
    const fetchMock = stubStore((index) => pieceOf(sealed, layout, index));
    await joinAndOpen(key, layout, sealed.length, 2);
    expect(fetchMock.mock.calls.map(([url]) => url)).toEqual([
      "/s/abc/segments/0?file=2",
      "/s/abc/segments/1?file=2",
      "/s/abc/segments/2?file=2",
      "/s/abc/segments/3?file=2",
    ]);
  });
});

const sealedFixture = async (count = 3) => {
  const key = newKey();
  const sealed = await seal(key, randomBytes(CHUNK_LENGTH * 3 + 1));
  return { key, layout: layoutOf(sealed.length, count), sealed };
};

describe("a join that must fail", () => {
  it("rejects the wrong key", async () => {
    const { layout, sealed } = await sealedFixture();
    stubStore((index) => pieceOf(sealed, layout, index));
    await expect(joinAndOpen(newKey(), layout, sealed.length)).rejects.toThrow(
      UNAUTHENTIC
    );
  });

  it("rejects a flipped byte in any piece", async () => {
    const { key, layout, sealed } = await sealedFixture();
    for (const target of [0, 1, 2]) {
      const tampered = Uint8Array.from(sealed);
      const at = (layout.segments[target]?.offset ?? 0) + 30;
      tampered[at] = ((tampered[at] ?? 0) + 1) % 256;
      stubStore((index) => pieceOf(tampered, layout, index));
      // oxlint-disable-next-line no-await-in-loop -- one stub at a time
      await expect(joinAndOpen(key, layout, sealed.length)).rejects.toThrow(
        UNAUTHENTIC
      );
    }
  });

  it("rejects pieces served in the wrong order", async () => {
    const { key, sealed } = await sealedFixture();
    // Equal-length pieces, so the swap passes the length check and only the
    // cipher can catch it.
    const stride = (sealed.length - 17 - 1 - 16) / 3;
    const layout: SegmentLayout = {
      segments: [
        { length: 17 + stride, offset: 0 },
        { length: stride, offset: 17 + stride },
        { length: stride, offset: 17 + stride * 2 },
        { length: 17, offset: 17 + stride * 3 },
      ],
      size_bytes: sealed.length,
    };
    const order = [0, 2, 1, 3];
    stubStore((index) => pieceOf(sealed, layout, order[index] ?? index));
    await expect(joinAndOpen(key, layout, sealed.length)).rejects.toThrow(
      UNAUTHENTIC
    );
  });

  it("rejects a piece that answers short", async () => {
    const { key, layout, sealed } = await sealedFixture();
    stubStore((index) => {
      const piece = pieceOf(sealed, layout, index);
      return index === 1 ? piece.subarray(0, -1) : piece;
    });
    await expect(joinAndOpen(key, layout, sealed.length)).rejects.toThrow(
      /^segment 1 ended at \d+ of \d+$/u
    );
  });

  it("rejects a piece that runs long", async () => {
    const { key, layout, sealed } = await sealedFixture();
    stubStore((index) => {
      const piece = pieceOf(sealed, layout, index);
      return index === 1 ? new Uint8Array([...piece, 0]) : piece;
    });
    await expect(joinAndOpen(key, layout, sealed.length)).rejects.toThrow(
      /^segment 1 ran long$/u
    );
  });

  it("lets openStream reject a layout that leaves out the last piece", async () => {
    // joinSegments has no total to check against: it stops after the pieces it
    // was told about. The crate's opener is what notices the missing tail.
    const { key, layout, sealed } = await sealedFixture();
    stubStore((index) => pieceOf(sealed, layout, index));
    const truncated = { ...layout, segments: layout.segments.slice(0, -1) };
    await expect(joinAndOpen(key, truncated, sealed.length)).rejects.toThrow(
      /^the sealed object ended early or ran long$/u
    );
  });

  it("rejects a sealed object cut at a chunk boundary and passed off as whole", async () => {
    const { key, sealed } = await sealedFixture();
    // Header and the first two full chunks only, with a size to match: no
    // chunk in there was sealed as the last.
    const stride = (sealed.length - 17 - 1 - 16) / 3;
    const cut = sealed.subarray(0, 17 + stride * 2);
    const layout = layoutOf(cut.length, 2);
    stubStore((index) => pieceOf(cut, layout, index));
    await expect(joinAndOpen(key, layout, cut.length)).rejects.toThrow(
      UNAUTHENTIC
    );
  });

  it("rejects a store that fails a later piece", async () => {
    const { key, layout, sealed } = await sealedFixture();
    vi.stubGlobal(
      "fetch",
      vi.fn<(input: string) => Promise<Response>>((input: string) =>
        Promise.resolve(
          input.endsWith("/2")
            ? new Response(null, { status: 403 })
            : new Response(
                pieceOf(sealed, layout, Number(input.at(-1))) as BodyInit
              )
        )
      )
    );
    await expect(joinAndOpen(key, layout, sealed.length)).rejects.toThrow(
      /segment 2 failed: 403/u
    );
  });
});

describe("fetchSegmentLayout", () => {
  it("returns null when the share cannot be fetched this way", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() => Promise.resolve(new Response(null, { status: 404 })))
    );
    expect(await fetchSegmentLayout("abc")).toBeNull();
  });

  it("asks for one file of several", async () => {
    const layout = layoutOf(10, 1);
    const fetchMock = vi.fn<() => Promise<Response>>(() =>
      Promise.resolve(Response.json(layout))
    );
    vi.stubGlobal("fetch", fetchMock);
    expect(await fetchSegmentLayout("a b", 2)).toEqual(layout);
    expect(fetchMock).toHaveBeenCalledWith(
      "/s/a%20b/segments?file=2",
      expect.objectContaining({ credentials: "include" })
    );
  });
});
