import { readFileSync } from "node:fs";
import { crc32 } from "node:zlib";

import { beforeAll, describe, expect, it } from "vite-plus/test";

import { MalformedImageError, readySync, stripMetadata } from "./index";

function chunk(type: string, data: Buffer) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "latin1"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([length, body, crc]);
}

/** A PNG with a `tEXt` chunk (metadata) between `IHDR` and `IEND`. */
function pngWithText() {
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk("IHDR", Buffer.from([0, 0, 0, 1, 0, 0, 0, 1, 8, 0, 0, 0, 0])),
    chunk("tEXt", Buffer.from("Comment\0secret")),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

describe("stripMetadata", () => {
  beforeAll(() => {
    readySync(
      readFileSync(new URL("wasm/tessera_metadata_bg.wasm", import.meta.url))
    );
  });

  it("leaves a file that is not a photo alone", async () => {
    const file = new File(["hello"], "note.txt", { type: "text/plain" });
    expect(await stripMetadata(file)).toBe(file);
  });

  it("removes a PNG's text chunk and keeps its name and type", async () => {
    const file = new File([pngWithText()], "a.png", { type: "image/png" });
    const cleaned = await stripMetadata(file);
    expect(cleaned).not.toBe(file);
    expect(cleaned.name).toBe("a.png");
    expect(cleaned.type).toBe("image/png");
    const bytes = Buffer.from(await cleaned.arrayBuffer());
    expect(bytes.includes("secret")).toBe(false);
    expect(bytes.length).toBeLessThan(file.size);
  });

  it("refuses a photo whose structure is broken", async () => {
    const broken = pngWithText().subarray(0, 40);
    const file = new File([broken], "b.png", { type: "image/png" });
    await expect(stripMetadata(file)).rejects.toBeInstanceOf(
      MalformedImageError
    );
  });
});
