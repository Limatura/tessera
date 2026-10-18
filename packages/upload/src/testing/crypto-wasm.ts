// Loads the @tessera/crypto WASM once per test file, from disk, since Node has
// no fetch for a relative module URL. Listed in vitest.config.ts setupFiles,
// the same arrangement as apps/web's.
import { readFileSync } from "node:fs";

import { readySync } from "@tessera/crypto";

readySync(
  readFileSync(
    new URL(
      "../../node_modules/@tessera/crypto/src/wasm/tessera_crypto_bg.wasm",
      import.meta.url
    )
  )
);
