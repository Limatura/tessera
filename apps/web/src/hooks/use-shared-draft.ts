import { useNavigate } from "@tanstack/react-router";
import { toast } from "@tessera/ui/components/toast";
import { useEffect, useState } from "react";

import type { SharedDraft } from "@/lib/pwa/shared-payload";
import { draftFromPayload, takeSharedPayload } from "@/lib/pwa/shared-payload";
import * as m from "@/paraglide/messages";

/**
 * Reading deletes the record, so a second read (StrictMode runs effects twice)
 * would find nothing. Both runs share the one read instead.
 */
const reads = new Map<string, Promise<SharedDraft | null>>();

function readDraft(id: string): Promise<SharedDraft | null> {
  let read = reads.get(id);
  if (!read) {
    read = takeDraft(id);
    reads.set(id, read);
  }
  return read;
}

async function takeDraft(id: string): Promise<SharedDraft | null> {
  try {
    const payload = await takeSharedPayload(id);
    return payload ? draftFromPayload(payload) : null;
  } catch {
    return null;
  }
}

/**
 * What the share sheet sent, for `/?share={id}` (see `public/sw.js`).
 *
 * The id is dropped from the address once read, so a reload or a copied link
 * does not try again.
 */
export function useSharedDraft(shareId: string | undefined) {
  const navigate = useNavigate();
  const [draft, setDraft] = useState<SharedDraft | null>(null);

  useEffect(() => {
    if (!shareId) {
      return;
    }
    let active = true;
    (async () => {
      const result = await readDraft(shareId);
      if (!active) {
        return;
      }
      setDraft(result);
      if (!result) {
        toast.add({
          description: m.upload_shared_missing_description(),
          title: m.upload_shared_missing(),
          type: "error",
        });
      }
      navigate({ replace: true, search: {}, to: "/" });
    })();
    return () => {
      active = false;
    };
  }, [navigate, shareId]);

  return [draft, () => setDraft(null)] as const;
}
