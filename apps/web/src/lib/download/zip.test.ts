import { describe, expect, it } from "vite-plus/test";

import { zipLength, zipStream } from "./zip";

const bytesOf = (text: string) => new TextEncoder().encode(text);

const streamOf = (...pieces: string[]) =>
  new ReadableStream<Uint8Array>({
    start: (controller) => {
      for (const piece of pieces) {
        controller.enqueue(bytesOf(piece));
      }
      controller.close();
    },
  });

async function collect(stream: ReadableStream<Uint8Array>) {
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

describe("zipStream", () => {
  const entries = [
    {
      name: "a.txt",
      open: () => Promise.resolve(streamOf("hello ", "world")),
      size: 11,
    },
    {
      name: "資料夾/b.txt",
      open: () => Promise.resolve(streamOf("")),
      size: 0,
    },
  ];

  it("is exactly as long as zipLength says", async () => {
    const zip = await collect(zipStream(entries));
    expect(zip.length).toBe(zipLength(entries));
  });

  it("writes the CRC-32 of each file in its descriptor", async () => {
    const zip = await collect(zipStream(entries));
    const view = new DataView(zip.buffer);
    // Local header (30) + "a.txt" (5) + content (11), then the descriptor
    const descriptor = 30 + 5 + 11;
    expect(view.getUint32(descriptor, true)).toBe(0x08_07_4b_50);
    // CRC-32 of "hello world"
    expect(view.getUint32(descriptor + 4, true)).toBe(0x0d_4a_11_85);
  });

  it("refuses a file shorter than its declared size", async () => {
    const short = [
      { name: "c", open: () => Promise.resolve(streamOf("ab")), size: 3 },
    ];
    await expect(collect(zipStream(short))).rejects.toThrow("ended early");
  });
});
