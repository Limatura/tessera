import { useQuery } from "@tanstack/react-query";
import { api } from "@tessera/api";
import { useEffect, useState } from "react";
import { useDebounce } from "use-debounce";

import { PASSWORD_MAX_CEILING, passwordRules } from "@/lib/api/password-policy";
import type { PasswordPolicy } from "@/lib/api/password-policy";

/** How long typing has to pause before the list is asked. */
const DEBOUNCE_MS = 400;

/** Characters of the hash that are sent. Fixed by the range API. */
const PREFIX_LENGTH = 5;

/** A prefix's answer does not change in the time it takes to fill a form. */
const RANGE_STALE_MS = 1000 * 60 * 10;

/**
 * Uppercase hex SHA-1, the spelling the range API answers in. `null` where
 * the browser offers no `crypto.subtle` — any page served over plain HTTP
 * from somewhere other than localhost. The form then goes without the
 * warning; submitting is where the check happens.
 */
async function sha1(value: string): Promise<string | null> {
  if (!globalThis.crypto?.subtle) {
    return null;
  }
  const digest = await crypto.subtle.digest(
    "SHA-1",
    new TextEncoder().encode(value)
  );
  return [...new Uint8Array(digest)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("")
    .toUpperCase();
}

/**
 * Whether `password` appears in a known breach, asked while it is typed.
 * Only the first five characters of its SHA-1 go to the server, with the
 * match made here, so the password stays in the browser until the form is
 * sent. Asked only once the instance's own rules pass — a password still too
 * short will change before it matters, and every lookup counts against this
 * address's allowance.
 *
 * `true` means breached. `false` covers "not in the list", "not asked yet"
 * and "could not ask": this is advance notice, and the server checks again
 * on submit regardless.
 */
export function usePasswordBreach(
  password: string,
  policy: PasswordPolicy
): boolean {
  const eligible =
    policy.checksBreaches &&
    [...password].length <= PASSWORD_MAX_CEILING &&
    passwordRules(policy, password).every((rule) => rule.satisfied);
  // What was typed before the last pause — only this is ever hashed and
  // asked about, so a lookup waits for somebody to stop typing.
  const [settled] = useDebounce(password, DEBOUNCE_MS);

  // Kept with the password it was computed for, so an answer about what was
  // typed a moment ago is never shown against what is typed now. The hash
  // itself stays out of the query key: the cache holds only the prefix.
  const [hashed, setHashed] = useState<{
    password: string;
    digest: string;
  } | null>(null);

  useEffect(() => {
    if (!(eligible && settled === password)) {
      return;
    }
    let cancelled = false;
    (async () => {
      const digest = await sha1(settled);
      if (!cancelled && digest) {
        setHashed({ digest, password: settled });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [eligible, password, settled]);

  const current = eligible && hashed?.password === password ? hashed : null;
  const prefix = current?.digest.slice(0, PREFIX_LENGTH);

  const { data: suffixes } = useQuery({
    enabled: Boolean(prefix),
    queryFn: async () => {
      const { data, error } = await api.GET("/api/v1/auth/pwned/{prefix}", {
        params: { path: { prefix: prefix ?? "" } },
      });
      if (error) {
        throw error;
      }
      return data.suffixes;
    },
    queryKey: ["pwned-range", prefix],
    retry: false,
    staleTime: RANGE_STALE_MS,
  });

  if (!(current && suffixes)) {
    return false;
  }
  return suffixes.includes(current.digest.slice(PREFIX_LENGTH));
}
