# End-to-end encryption

A share can be sealed before it leaves the uploader's machine, so the server stores and hands out bytes it holds no key for. The format lives in `crates/tessera-crypto`, and the browser and the command line run that same crate: natively in one, compiled to WebAssembly in the other.

## Who it protects against

**The operator, and anyone who gets what the operator has.** A database dump, a copy of the bucket, a storage provider reading its customers' objects: all of them see sealed bytes.

It does nothing for an instance someone runs for themselves, where the operator is the only person it could keep out. It is for instances run for people the operator does not know. For their visitors it is a guarantee; for the operator it is a position they can hold: they cannot read what is stored.

**Not against a page that lies.** In a browser, the code doing the sealing is JavaScript the same server just sent. An operator who wanted keys could serve a page that sends them home, and no web application can rule that out from inside itself. What it does rule out is the passive case: stored data read later, by the operator or by whoever took a copy. The command line does not have this gap, since its code did not come from the server it talks to.

## The two ways to hold a key

**In the link.** The key is 32 random bytes, written after `#` in the share's link. A browser never sends the part after `#` to a server, so the request for the page carries the code and not the key. Whoever has the whole link can open the share; whoever has only the retrieval code cannot.

**From a password.** The key is derived with Argon2id from a password and a random salt the server stores. The link carries no key, so the code and the password are enough, and both can be read out over the phone. The server is handed a token derived from the key, not the password, and uses it the way it uses any share password: a wrong one is refused and throttled, and retrievals are counted. The token opens nothing.

The price is that whoever has the stored bytes can guess passwords offline, as fast as Argon2id lets them. The server never sees the password, so it cannot refuse a weak one; the client has to. For the same reason the password of such a share cannot be changed or removed: the key comes from it.

## What is hidden, and what is not

Hidden: the content, file names and media types, the share's title, and a text share's language.

Not hidden: how large each file is (to within the chunking overhead), how many files there are, when the share was made, when it expires, how often it is fetched and from where. The server needs those to enforce quotas, expiry and limits.

## The format

A sealed object is a header and the content in chunks:

```text
version (1 byte, 0x01) ‖ nonce (16 bytes) ‖ chunk₀ ‖ chunk₁ ‖ … ‖ chunkₙ₋₁
```

- **Per-object key:** HKDF-SHA256 with the nonce as salt, the share key as input and `tessera v1 payload` as info. A fresh nonce per object means no two objects share a key.
- **Chunks:** 64 KiB of content each, sealed with ChaCha20-Poly1305, so each is 16 bytes longer. Every chunk but the last is full; the last holds at least one byte, unless the whole content is empty and there is one empty chunk.
- **Chunk nonce:** the chunk's index as 11 big-endian bytes, then `0x01` for the last chunk and `0x00` for any other. This is the STREAM construction `age` uses. The flag is what makes an object cut short on a chunk boundary fail to open, instead of opening as a shorter file.
- **Password:** Argon2id, 64 MiB, 3 passes, 1 lane, 32 bytes out, over the password's UTF-8 bytes (not normalised) and a 16-byte salt. The parameters are fixed by the version rather than stored per share.
- **Access token:** HKDF-SHA256 with no salt, the share key as input and `tessera v1 access` as info.
- **Encodings:** the key, salt and token are unpadded URL-safe base64.

Because every chunk but the last is the same size, a sealed length fixes the content length, and chunk `i` starts at `17 + i × 65552`. A `Range` request can fetch and open one chunk without anything before it; `Opener::locate` does the arithmetic.

`the_format_has_not_moved` pins a sealed object and an access token, and `a_password_derives_the_same_key_every_time` pins a derived key. Both values were also produced independently, with Python's `cryptography` for HKDF and ChaCha20-Poly1305 and with the Argon2 reference implementation through `argon2-cffi`. A change to either test is a new format version.

## Why ChaCha20-Poly1305 and not AES-GCM

Browsers have AES-GCM built in through WebCrypto, but WebCrypto has no Argon2, so the password half needs WebAssembly regardless. Once WebAssembly is there, one implementation for everything beats two. The question was whether it is fast enough.

WebAssembly cannot use the processor's AES instructions, so AES in it is slow; ChaCha20 was designed to be fast without them. Measured on a Ryzen 7 8700G, sealing 256 MiB in 1 MiB pieces:

| MB/s | Chromium 151 | Firefox 155 | WebKit 26.5 |
| --- | --- | --- | --- |
| WebCrypto AES-GCM | 809 | 705 | 1842 |
| WebAssembly AES-GCM | 56 | 49 | 41 |
| WebAssembly ChaCha20-Poly1305 | 391 | 316 | 382 |

AES in WebAssembly cannot keep up with a gigabit link. ChaCha20 is two to five times slower than WebCrypto and still faster than the network it feeds. Argon2id at the parameters above took 129 to 164 ms in the same three engines.

Phones were not measured. A phone's upload rarely exceeds a few megabytes a second, so encryption would have to be a hundred times slower than on the desktop before it set the pace.
