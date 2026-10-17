// Loads the @tessera/crypto WASM once per test file, from disk, since Node has
// no fetch for a relative module URL. Listed in vitest.config.ts setupFiles.
//
// It runs for every test file, but costs a few milliseconds: compiling the
// module is lazy in V8 and the instance is a few hundred KB. A separate Vitest
// project only for the crypto tests would cost more in config than it saves.
import { readFileSync } from "node:fs";

import { readySync } from "@tessera/crypto";

readySync(
  readFileSync(
    new URL(
      "../../../node_modules/@tessera/crypto/src/wasm/tessera_crypto_bg.wasm",
      import.meta.url
    )
  )
);
