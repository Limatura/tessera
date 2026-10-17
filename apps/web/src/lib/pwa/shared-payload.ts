/**
 * What the system share sheet handed the installed app.
 *
 * `public/sw.js` answers `POST /share-target`, stores the form in IndexedDB
 * under a random id and opens `/?share={id}`. The page takes it from there; the
 * record is deleted as it is read, so a reload does not prefill it twice.
 */
export interface SharedPayload {
  files: File[];
  title: string;
  text: string;
  url: string;
}

// Must match `public/sw.js`.
const SHARE_DB = "tessera-share";
const SHARE_STORE = "payloads";

/** Reads and deletes the payload, or `null` if it is gone (or never existed). */
export async function takeSharedPayload(
  id: string
): Promise<SharedPayload | null> {
  if (typeof indexedDB === "undefined") {
    return null;
  }
  const db = await openShareDb();
  try {
    // oxlint-disable-next-line promise/avoid-new -- IndexedDB only has events
    return await new Promise((resolve, reject) => {
      const tx = db.transaction(SHARE_STORE, "readwrite");
      const store = tx.objectStore(SHARE_STORE);
      const read = store.get(id);
      let payload: SharedPayload | null = null;
      read.addEventListener("success", () => {
        payload = (read.result as SharedPayload | undefined) ?? null;
        store.delete(id);
      });
      tx.addEventListener("complete", () => resolve(payload));
      tx.addEventListener("error", () => reject(tx.error));
      tx.addEventListener("abort", () => reject(tx.error));
    });
  } finally {
    db.close();
  }
}

function openShareDb(): Promise<IDBDatabase> {
  // oxlint-disable-next-line promise/avoid-new -- IndexedDB only has events
  return new Promise((resolve, reject) => {
    const open = indexedDB.open(SHARE_DB, 1);
    open.addEventListener("upgradeneeded", () => {
      open.result.createObjectStore(SHARE_STORE);
    });
    open.addEventListener("success", () => resolve(open.result));
    open.addEventListener("error", () => reject(open.error));
  });
}

/** Which upload tab a share belongs in, with what to put there. */
export type SharedDraft =
  | { kind: "file"; files: File[] }
  | { kind: "link"; url: string }
  | { kind: "code"; content: string; title?: string };

/**
 * Picks the tab for a share.
 *
 * Files win: a photo shared with a caption is a file share. A URL alone is a
 * link share; Android apps disagree on whether it goes in `url` or `text`, so a
 * `text` that is nothing but a URL counts too. Anything else is text for the
 * code tab, with the URL (if any) kept on its own line.
 */
export function draftFromPayload(payload: SharedPayload): SharedDraft | null {
  if (payload.files.length > 0) {
    return { files: payload.files, kind: "file" };
  }
  const text = payload.text.trim();
  const url = payload.url.trim() || (isUrl(text) ? text : "");
  if (url && (text === "" || text === url)) {
    return { kind: "link", url };
  }
  const content = [text, url].filter(Boolean).join("\n");
  if (content === "") {
    return null;
  }
  const title = payload.title.trim();
  return { content, kind: "code", ...(title ? { title } : {}) };
}

function isUrl(value: string): boolean {
  if (/\s/u.test(value)) {
    return false;
  }
  try {
    const { protocol } = new URL(value);
    return protocol === "http:" || protocol === "https:";
  } catch {
    return false;
  }
}
