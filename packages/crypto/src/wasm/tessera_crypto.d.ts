/* tslint:disable */
/* eslint-disable */

/**
 * Opens the chunks of one object, in any order. See [`crate::Opener`].
 */
export class Opener {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * How many chunks the object has.
     */
    chunks(): bigint;
    /**
     * How many sealed bytes chunk `index` is.
     *
     * # Errors
     *
     * Past the last chunk.
     */
    length(index: bigint): bigint;
    /**
     * Prepares to open an object of `sealed_len` bytes starting with `header`.
     *
     * # Errors
     *
     * As [`crate::Opener::new`].
     */
    constructor(key: ShareKey, header: Uint8Array, sealed_len: bigint);
    /**
     * Opens chunk `index`.
     *
     * # Errors
     *
     * As [`crate::Opener::open`].
     */
    open(index: bigint, chunk: Uint8Array): Uint8Array;
    /**
     * Where chunk `index` starts in the sealed object.
     *
     * # Errors
     *
     * Past the last chunk.
     */
    start(index: bigint): bigint;
}

/**
 * Seals content one chunk at a time. See [`crate::Sealer`].
 */
export class Sealer {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * What goes in front of the first chunk.
     */
    header(): Uint8Array;
    /**
     * Starts an object with 16 bytes of `crypto.getRandomValues` as its nonce.
     *
     * # Errors
     *
     * When there are not exactly 16.
     */
    constructor(key: ShareKey, nonce: Uint8Array);
    /**
     * Seals the next chunk.
     *
     * # Errors
     *
     * As [`crate::Sealer::seal`].
     */
    seal(chunk: Uint8Array, last: boolean): Uint8Array;
}

/**
 * The secret a share is sealed under. See [`crate::ShareKey`].
 */
export class ShareKey {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * What to present as the share's password, when the key came from one.
     */
    accessToken(): string;
    /**
     * The key as it goes after `#` in a link.
     */
    encode(): string;
    /**
     * The key a password and an encoded salt derive. Takes a noticeable
     * fraction of a second, by design.
     *
     * # Errors
     *
     * When the salt does not decode.
     */
    static fromPassword(password: string, salt: string): ShareKey;
    /**
     * A key from 32 bytes of `crypto.getRandomValues`.
     *
     * # Errors
     *
     * When there are not exactly 32.
     */
    static fromRandom(bytes: Uint8Array): ShareKey;
    /**
     * Reads a key as it appears after `#` in a link.
     *
     * # Errors
     *
     * When it is not one.
     */
    static parse(encoded: string): ShareKey;
}

/**
 * How much content one chunk holds, for a caller cutting a file up.
 */
export function chunkLength(): number;

/**
 * Encodes 16 bytes of `crypto.getRandomValues` as a salt.
 *
 * # Errors
 *
 * When there are not exactly 16.
 */
export function encodeSalt(bytes: Uint8Array): string;

/**
 * The shortest password a key may come from. See
 * [`crate::MIN_PASSWORD_CHARS`].
 */
export function minimumPasswordLength(): number;

/**
 * Opens what [`seal`] sealed.
 *
 * # Errors
 *
 * When it does not open.
 */
export function open(key: ShareKey, sealed: Uint8Array): Uint8Array;

/**
 * How long the content in `sealed_len` sealed bytes is.
 *
 * # Errors
 *
 * For a length no object can have.
 */
export function plainLength(sealed_len: bigint): bigint;

/**
 * Seals something small at once. See [`crate::seal`].
 *
 * # Errors
 *
 * When the nonce is not 16 bytes.
 */
export function seal(key: ShareKey, nonce: Uint8Array, content: Uint8Array): Uint8Array;

/**
 * How long `plain_len` bytes are once sealed.
 *
 * # Errors
 *
 * Past what a `u64` can say.
 */
export function sealedLength(plain_len: bigint): bigint;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_opener_free: (a: number, b: number) => void;
    readonly __wbg_sealer_free: (a: number, b: number) => void;
    readonly __wbg_sharekey_free: (a: number, b: number) => void;
    readonly chunkLength: () => number;
    readonly encodeSalt: (a: number, b: number, c: number) => void;
    readonly minimumPasswordLength: () => number;
    readonly open: (a: number, b: number, c: number, d: number) => void;
    readonly opener_chunks: (a: number) => bigint;
    readonly opener_length: (a: number, b: number, c: bigint) => void;
    readonly opener_new: (a: number, b: number, c: number, d: number, e: bigint) => void;
    readonly opener_open: (a: number, b: number, c: bigint, d: number, e: number) => void;
    readonly opener_start: (a: number, b: number, c: bigint) => void;
    readonly plainLength: (a: number, b: bigint) => void;
    readonly seal: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly sealedLength: (a: number, b: bigint) => void;
    readonly sealer_header: (a: number, b: number) => void;
    readonly sealer_new: (a: number, b: number, c: number, d: number) => void;
    readonly sealer_seal: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly sharekey_accessToken: (a: number, b: number) => void;
    readonly sharekey_encode: (a: number, b: number) => void;
    readonly sharekey_fromPassword: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly sharekey_fromRandom: (a: number, b: number, c: number) => void;
    readonly sharekey_parse: (a: number, b: number, c: number) => void;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export: (a: number, b: number) => number;
    readonly __wbindgen_export2: (a: number, b: number, c: number) => void;
    readonly __wbindgen_export3: (a: number, b: number, c: number, d: number) => number;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
