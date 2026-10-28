/**
 * Removes the metadata from a photo before it is uploaded: where it was taken,
 * the camera, the time, comments.
 *
 * The work is Rust, in `crates/tessera-metadata`, compiled to the WebAssembly
 * module in `./wasm` by `scripts/crypto-wasm.sh`. The command line runs the
 * same crate natively, so both remove the same things. What is here only
 * reads a `File` into it and wraps the answer in another.
 *
 * Call {@link ready} once and await it before {@link stripMetadata}.
 */
import init, { handles, initSync, strip } from "./wasm/tessera_metadata.js";

/** Bytes that tell a JPEG, PNG or WebP apart; the crate needs twelve. */
const HEAD_LENGTH = 12;

let loading: Promise<unknown> | undefined;

/**
 * Loads the module. Safe to call more than once; every call waits for the same
 * load. Without an argument it fetches the `.wasm` beside this file, which
 * Vite emits as an asset.
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

/** A file that looks like a photo but whose structure is broken. */
export class MalformedImageError extends Error {
  readonly fileName: string;

  constructor(fileName: string, reason: string) {
    super(`${fileName}: ${reason}`);
    this.name = "MalformedImageError";
    this.fileName = fileName;
  }
}

/**
 * The file without its metadata. The same file when it is not a JPEG, PNG or
 * WebP photo or carries none, so a caller can pass everything through. The
 * orientation stays, so a photo still shows the right way up.
 *
 * @throws {MalformedImageError} When it starts like a photo but is not a
 * well-formed one, so its metadata cannot be found reliably. Better to refuse
 * than to upload a location the uploader thought was gone.
 */
export async function stripMetadata(file: File): Promise<File> {
  await ready();
  const head = new Uint8Array(await file.slice(0, HEAD_LENGTH).arrayBuffer());
  if (!handles(head)) {
    return file;
  }
  let cleaned: Uint8Array | undefined;
  try {
    cleaned = strip(new Uint8Array(await file.arrayBuffer()));
  } catch (error) {
    throw new MalformedImageError(
      file.name,
      error instanceof Error ? error.message : String(error)
    );
  }
  if (cleaned === undefined) {
    return file;
  }
  // Copied out of the module's realm: a `File` built from another realm's
  // typed array (a test runner's sandbox) takes it for a string.
  return new File([new Uint8Array(cleaned)], file.name, {
    lastModified: file.lastModified,
    type: file.type,
  });
}
