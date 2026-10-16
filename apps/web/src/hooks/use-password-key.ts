import { useQueryClient } from "@tanstack/react-query";
import type { ShareKey } from "@tessera/crypto";
import { keyFromPassword, ready } from "@tessera/crypto";
import { useState } from "react";

import { shareQueryOptions } from "@/hooks/use-share";

/**
 * Turns what the visitor typed into the share password to present.
 *
 * For a share whose key comes from a password (`encryption=password`), the
 * server never sees the password: the key is derived here from the salt the
 * server stored, and the key's access token is what unlocks. The locked
 * description, the first lookup without a password, is where the salt is.
 * Any other share takes the password as typed.
 *
 * Deriving blocks the page for a fraction of a second, more on a phone, so
 * `deriving` is set and painted first.
 */
export function usePasswordKey(
  code: string,
  setPassword: (password: string) => void
) {
  const queryClient = useQueryClient();
  const [key, setKey] = useState<ShareKey>();
  const [deriving, setDeriving] = useState(false);

  const submit = async (typed: string) => {
    const locked = queryClient.getQueryData(shareQueryOptions(code).queryKey);
    if (locked?.encryption !== "password" || !locked.key_salt) {
      setPassword(typed);
      return;
    }
    setDeriving(true);
    try {
      await ready();
      await nextFrame();
      const derived = keyFromPassword(typed, locked.key_salt);
      setKey(derived);
      setPassword(derived.accessToken());
    } finally {
      setDeriving(false);
    }
  };

  return { deriving, key, submit };
}

function nextFrame() {
  // oxlint-disable-next-line promise/avoid-new -- requestAnimationFrame only takes a callback
  return new Promise<void>((resolve) => {
    requestAnimationFrame(() => setTimeout(resolve, 0));
  });
}
