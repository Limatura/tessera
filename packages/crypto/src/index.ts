/**
 * End-to-end encryption for shares, in the browser.
 *
 * The format itself is Rust, in `crates/tessera-crypto`, compiled to the
 * WebAssembly module in `./wasm` by `scripts/crypto-wasm.sh`. The command line
 * runs the same crate natively, which is how the two cannot disagree about a
 * file. What is here turns that into what a page needs: random keys, streams,
 * and strings that fit in a URL or a query parameter.
 *
 * Call {@link ready} once and await it before anything else.
 */
import init, {
  encodeSalt,
  initSync,
  open,
  Opener,
  plainLength,
  Sealer,
  seal,
  sealedLength,
  ShareKey,
} from "./wasm/tessera_crypto.js";

export { Opener, ShareKey } from "./wasm/tessera_crypto.js";

/** How much content one chunk holds. The same as the crate's `CHUNK_LEN`. */
export const CHUNK_LENGTH = 64 * 1024;

/** How long the header in front of a sealed object is. */
export const HEADER_LENGTH = 17;

/**
 * The shortest password a key may come from. The server never sees the
 * password, so it cannot refuse a weak one: a client has to. The same as the
 * crate's `MIN_PASSWORD_CHARS`, counted in characters, not bytes.
 */
export const MIN_PASSWORD_LENGTH = 12;

/** A file's name and media type, as `sealed_name` holds them. */
export interface SealedName {
  /** What the file was called, with the folders it sat in separated by `/`. */
  name: string;
  /** Its media type, when known. */
  type?: string;
}

/** A share's title and a note's language, as `sealed_details` holds them. */
export interface SealedDetails {
  title?: string;
  language?: string;
}

let loading: Promise<unknown> | undefined;

/**
 * Loads the module. Safe to call more than once; every call waits for the same
 * load.
 *
 * Without an argument it fetches the `.wasm` beside this file, which a bundler
 * that understands `new URL(..., import.meta.url)` (Vite does) emits as an
 * asset.
 */
export async function ready(source?: URL | Response | BufferSource) {
  loading ??= init(
    source === undefined ? undefined : { module_or_path: source }
  );
  await loading;
}

/** Loads the module from bytes already in hand, for a test or a worker. */
export function readySync(bytes: BufferSource) {
  initSync({ module: bytes });
  loading ??= Promise.resolve();
}

function random(length: number) {
  return crypto.getRandomValues(new Uint8Array(length));
}

/** A fresh key for a share whose key travels in its link. */
export function newKey() {
  return ShareKey.fromRandom(random(32));
}

/** A fresh salt for a share whose key comes from a password. */
export function newSalt() {
  return encodeSalt(random(16));
}

/**
 * The key a password derives. Deliberately slow: a fraction of a second on a
 * desktop, more on a phone, so run it where the page can show that it is busy.
 */
export function keyFromPassword(password: string, salt: string) {
  return ShareKey.fromPassword(password, salt);
}

/** How many bytes `size` bytes of content take once sealed. */
export function sealedSize(size: number) {
  return Number(sealedLength(BigInt(size)));
}

/** How many bytes of content `size` sealed bytes hold. Throws for a size no sealed object has. */
export function plainSize(size: number) {
  return Number(plainLength(BigInt(size)));
}

/** Seals something small at once: a note, or a name before it is encoded. */
export function sealBytes(key: ShareKey, content: Uint8Array) {
  return seal(key, random(16), content);
}

/** Opens what {@link sealBytes} sealed. Throws if it was altered or the key is wrong. */
export function openBytes(key: ShareKey, sealed: Uint8Array) {
  return open(key, sealed);
}

/**
 * Seals a string into what the API takes as `sealed_name` or `sealed_details`:
 * unpadded URL-safe base64. The string is typically JSON of whatever the page
 * wants back, since the server never looks inside.
 */
export function sealText(key: ShareKey, text: string) {
  return toBase64Url(sealBytes(key, new TextEncoder().encode(text)));
}

/** Opens what {@link sealText} sealed. */
export function openText(key: ShareKey, sealed: string) {
  return new TextDecoder().decode(openBytes(key, fromBase64Url(sealed)));
}

/** Seals a file's name into `sealed_name`. */
export function sealName(key: ShareKey, name: SealedName) {
  return sealText(key, JSON.stringify(name));
}

/** Opens a `sealed_name`. */
export function openName(key: ShareKey, sealed: string): SealedName {
  return JSON.parse(openText(key, sealed)) as SealedName;
}

/** Seals a share's title and language into `sealed_details`. */
export function sealDetails(key: ShareKey, details: SealedDetails) {
  return sealText(key, JSON.stringify(details));
}

/** Opens a `sealed_details`. */
export function openDetails(key: ShareKey, sealed: string): SealedDetails {
  return JSON.parse(openText(key, sealed)) as SealedDetails;
}

/**
 * Seals whatever flows through, in pieces of any size, into one sealed object.
 *
 * The output is {@link sealedSize} of the input's size, so an upload can declare
 * its length before the first byte.
 */
export function sealStream(key: ShareKey) {
  const sealer = new Sealer(key, random(16));
  const buffer = new Uint8Array(CHUNK_LENGTH);
  let filled = 0;

  return new TransformStream<Uint8Array, Uint8Array>({
    start(controller) {
      controller.enqueue(sealer.header());
    },
    transform(piece, controller) {
      let offset = 0;
      while (offset < piece.length) {
        // A full chunk is sealed only once more content arrives: until then it
        // might be the last, and the last is sealed differently.
        if (filled === CHUNK_LENGTH) {
          controller.enqueue(sealer.seal(buffer, false));
          filled = 0;
        }
        const taken = Math.min(CHUNK_LENGTH - filled, piece.length - offset);
        buffer.set(piece.subarray(offset, offset + taken), filled);
        filled += taken;
        offset += taken;
      }
    },
    flush(controller) {
      controller.enqueue(sealer.seal(buffer.subarray(0, filled), true));
      sealer.free();
    },
  });
}

/**
 * Opens a sealed object as it flows through, given its full sealed size.
 *
 * The size is what says which chunk is last. An object that ends early throws
 * rather than yielding a shorter file: the chunk that is now last was not
 * sealed as the last.
 */
export function openStream(key: ShareKey, size: number) {
  const pending: Uint8Array[] = [];
  let buffered = 0;
  let opener: Opener | undefined;
  let index = 0n;

  const take = (length: number) => {
    const out = new Uint8Array(length);
    let written = 0;
    while (written < length) {
      const head = pending[0] as Uint8Array;
      const used = Math.min(head.length, length - written);
      out.set(head.subarray(0, used), written);
      written += used;
      if (used === head.length) {
        pending.shift();
      } else {
        pending[0] = head.subarray(used);
      }
    }
    buffered -= length;
    return out;
  };

  const drain = (controller: TransformStreamDefaultController<Uint8Array>) => {
    if (!opener) {
      if (buffered < HEADER_LENGTH) {
        return;
      }
      opener = new Opener(key, take(HEADER_LENGTH), BigInt(size));
    }
    while (index < opener.chunks()) {
      const length = Number(opener.length(index));
      if (buffered < length) {
        return;
      }
      controller.enqueue(opener.open(index, take(length)));
      index += 1n;
    }
  };

  return new TransformStream<Uint8Array, Uint8Array>({
    transform(piece, controller) {
      pending.push(piece);
      buffered += piece.length;
      drain(controller);
    },
    flush(controller) {
      drain(controller);
      if (!opener || index < opener.chunks() || buffered > 0) {
        throw new Error("the sealed object ended early or ran long");
      }
      opener.free();
    },
  });
}

function toBase64Url(bytes: Uint8Array) {
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCodePoint(byte);
  }
  return btoa(binary)
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/u, "");
}

function fromBase64Url(text: string) {
  const binary = atob(text.replaceAll("-", "+").replaceAll("_", "/"));
  // Each character of what `atob` returns is one byte, 0 to 255.
  return Uint8Array.from(binary, (char) => char.codePointAt(0) ?? 0);
}
