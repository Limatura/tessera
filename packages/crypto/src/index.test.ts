import { readFileSync } from "node:fs";

import { beforeAll, describe, expect, it } from "vite-plus/test";

import {
  CHUNK_LENGTH,
  MIN_PASSWORD_LENGTH,
  openName,
  keyFromPassword,
  newKey,
  openStream,
  openText,
  readySync,
  sealStream,
  sealName,
  sealText,
  sealedSize,
  ShareKey,
} from "./index";
import {
  chunkLength,
  minimumPasswordLength,
  seal,
} from "./wasm/tessera_crypto.js";

beforeAll(() => {
  readySync(
    readFileSync(new URL("wasm/tessera_crypto_bg.wasm", import.meta.url))
  );
});

const hex = (bytes: Uint8Array) =>
  Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");

const pieces = (bytes: Uint8Array, sizes: number[]) => {
  const out: Uint8Array[] = [];
  let offset = 0;
  let turn = 0;
  while (offset < bytes.length) {
    const size = sizes[turn % sizes.length] as number;
    out.push(bytes.subarray(offset, offset + size));
    offset += size;
    turn += 1;
  }
  return out;
};

const through = async (
  input: Uint8Array[],
  stream: TransformStream<Uint8Array, Uint8Array>
) => {
  const source = new ReadableStream<Uint8Array>({
    start(controller) {
      for (const piece of input) {
        controller.enqueue(piece);
      }
      controller.close();
    },
  });
  return new Uint8Array(
    await new Response(source.pipeThrough(stream)).arrayBuffer()
  );
};

describe("the module is the crate's format", () => {
  // The values the Rust tests pin, which were also produced independently.
  // If these hold, a file sealed here opens on the command line.
  it("seals the pinned vector", () => {
    const key = ShareKey.fromRandom(
      Uint8Array.from({ length: 32 }, (_, i) => i)
    );
    const nonce = Uint8Array.from({ length: 16 }, (_, i) => 0xa0 + i);
    expect(hex(seal(key, nonce, new TextEncoder().encode("tessera")))).toBe(
      "01a0a1a2a3a4a5a6a7a8a9aaabacadaeafff159ab0bc53fd3de9a5f988a30c7dc89876256fac4ab9"
    );
    expect(key.accessToken()).toBe(
      "DB3xOr4CmNWw6gamlTsJanup3mgZq1ZD17GowkvifhE"
    );
  });

  it("derives the pinned key from a password", () => {
    const key = keyFromPassword(
      "correct horse battery staple",
      "AwMDAwMDAwMDAwMDAwMDAw"
    );
    expect(key.encode()).toBe("c-t0FiYWQY1kPwjcCFZTnqYUAMsmj4XOjfAdgld5W40");
  });

  it("agrees with the crate about the chunk size and the password minimum", () => {
    expect(CHUNK_LENGTH).toBe(chunkLength());
    expect(MIN_PASSWORD_LENGTH).toBe(minimumPasswordLength());
  });
});

describe("streams", () => {
  const sizes = [
    0,
    1,
    CHUNK_LENGTH - 1,
    CHUNK_LENGTH,
    CHUNK_LENGTH + 1,
    3 * CHUNK_LENGTH + 5,
  ];

  it.each(sizes)(
    "round-trips %i bytes arriving in uneven pieces",
    async (size) => {
      const content = Uint8Array.from(
        { length: size },
        (_, i) => (i * 7) % 251
      );
      const key = newKey();

      const sealed = await through(
        pieces(content, [1000, 70_000, 3]),
        sealStream(key)
      );
      expect(sealed.length).toBe(sealedSize(size));

      const opened = await through(
        pieces(sealed, [65_553, 17, 9]),
        openStream(key, sealed.length)
      );
      expect(hex(opened)).toBe(hex(content));
    }
  );

  // Cut on a chunk boundary, and the length claimed to match: what a server
  // lying about the size would send. The flag on the last chunk catches it.
  it("refuses an object cut short and passed off as whole", async () => {
    const key = newKey();
    const sealed = await through(
      [new Uint8Array(3 * CHUNK_LENGTH)],
      sealStream(key)
    );
    const cut = sealed.subarray(0, 17 + 2 * (CHUNK_LENGTH + 16));
    await expect(through([cut], openStream(key, cut.length))).rejects.toThrow(
      "the content was altered, or the key is wrong"
    );
  });

  it("refuses an object that stops before the size it was promised", async () => {
    const key = newKey();
    const sealed = await through([new Uint8Array(100)], sealStream(key));
    await expect(
      through([sealed.subarray(0, 50)], openStream(key, sealed.length))
    ).rejects.toThrow("the sealed object ended early or ran long");
  });

  it("refuses the wrong key", async () => {
    const sealed = await through([new Uint8Array(10)], sealStream(newKey()));
    await expect(
      through([sealed], openStream(newKey(), sealed.length))
    ).rejects.toThrow("the content was altered, or the key is wrong");
  });
});

describe("descriptions", () => {
  it("seal to something a query parameter carries unescaped, and open back", () => {
    const key = newKey();
    const name = JSON.stringify({
      name: "報告 final?.pdf",
      type: "application/pdf",
    });
    const sealed = sealText(key, name);
    expect(sealed).toMatch(/^[A-Za-z0-9_-]+$/u);
    expect(openText(key, sealed)).toBe(name);
  });

  // The JSON inside is an agreement with the command line, which writes and
  // reads `{"name": ..., "type": ...}`: the field is `type`, not `mediaType`.
  it("seal a name as the JSON the command line reads", () => {
    const key = newKey();
    const sealed = sealName(key, {
      name: "photos/cat.jpg",
      type: "image/jpeg",
    });
    expect(openText(key, sealed)).toBe(
      '{"name":"photos/cat.jpg","type":"image/jpeg"}'
    );
    expect(openName(key, sealed).name).toBe("photos/cat.jpg");
  });

  it("carry a key through a link", () => {
    const key = newKey();
    expect(ShareKey.parse(key.encode()).encode()).toBe(key.encode());
  });
});
