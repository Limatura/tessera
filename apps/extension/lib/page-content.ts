import {
  detectLanguage,
  languageFromClassName,
  languageFromPath,
} from "@tessera/upload/languages";

/**
 * Getting what the user right-clicked on out of the page: the media behind an
 * element, a linked file, or the selected text.
 */
import { browser } from "#imports";

/** Where the click happened, so a fallback can run inside that frame. */
export interface ClickedIn {
  tabId: number;
  frameId: number;
}

/**
 * The largest file copied out of the page itself. That route passes the bytes
 * as text (an injected script's result must be serialisable), which costs
 * about three times the file in memory; anything bigger needs the "media from
 * any site" permission, which lets the extension fetch it directly.
 */
const MAX_IN_PAGE_BYTES = 64 * 1024 * 1024;

/** Why the clicked media could not be had; each has its own advice. */
export class MediaError extends Error {
  override name = "MediaError";
  readonly reason: "blocked" | "too-large" | "stream" | "http";

  constructor(reason: MediaError["reason"], detail?: string) {
    super(detail ?? reason);
    this.reason = reason;
  }
}

/**
 * Downloads `url` as a file.
 *
 * Fetched by the extension first: it may reach any origin it holds a
 * permission for (the clicked tab's, through `activeTab`, or every one when
 * the user allowed it), with the user's cookies, so an image behind a login
 * still comes through. When that is refused (a CDN on another origin with no
 * CORS for an extension), the page that shows it fetches it instead, as it
 * already did to display it. A `blob:` URL only exists inside its page, so it
 * goes there directly.
 */
export async function fetchMedia(
  url: string,
  clickedIn: ClickedIn | undefined,
  signal: AbortSignal
): Promise<File> {
  if (url.startsWith("blob:")) {
    return fetchInPage(url, clickedIn);
  }
  const response = await fetchDirect(url, signal);
  if (response) {
    if (!response.ok) {
      throw new MediaError("http", String(response.status));
    }
    const blob = await response.blob();
    return new File([blob], fileName(url, response.headers, blob.type), {
      type: blob.type,
    });
  }
  return fetchInPage(url, clickedIn);
}

/** `undefined` when the request could not be made at all (CORS, offline). */
async function fetchDirect(
  url: string,
  signal: AbortSignal
): Promise<Response | undefined> {
  // With cookies first: with a host permission, CORS does not apply, and the
  // cookies are what a login-protected image needs. Without one, a server
  // answering `Access-Control-Allow-Origin: *` refuses a request carrying
  // credentials, so it gets a second try without.
  for (const credentials of ["include", "omit"] as const) {
    try {
      // oxlint-disable-next-line no-await-in-loop -- the second try only runs when the first failed
      return await fetch(url, { credentials, signal });
    } catch (error) {
      if (signal.aborted) {
        throw error;
      }
    }
  }
  return undefined;
}

interface InPageResult {
  dataUrl?: string;
  name?: string;
  type?: string;
  error?: "blocked" | "too-large" | "stream" | "http";
  status?: number;
}

async function fetchInPage(
  url: string,
  clickedIn: ClickedIn | undefined
): Promise<File> {
  if (!clickedIn) {
    throw new MediaError("blocked");
  }
  let injection;
  try {
    [injection] = await browser.scripting.executeScript({
      args: [url, MAX_IN_PAGE_BYTES],
      func: readInPage,
      target: { frameIds: [clickedIn.frameId], tabId: clickedIn.tabId },
    });
  } catch {
    // A page the extension may not script (the store, browser pages).
    throw new MediaError("blocked");
  }
  const result = injection?.result as InPageResult | undefined;
  if (!result?.dataUrl) {
    throw new MediaError(
      result?.error ?? "blocked",
      result?.status?.toString()
    );
  }
  const decoded = await fetch(result.dataUrl);
  const blob = await decoded.blob();
  const type = result.type || blob.type;
  return new File([blob], result.name || fileName(url, new Headers(), type), {
    type,
  });
}

/**
 * Runs inside the page, so it cannot use anything from this module: it is
 * serialised and sent there on its own.
 */
async function readInPage(
  url: string,
  maxBytes: number
): Promise<InPageResult> {
  let response: Response;
  try {
    response = await fetch(url);
  } catch {
    // A `blob:` URL of a MediaSource (a streamed video) cannot be fetched;
    // anything else that throws instead of answering is CORS refusing it.
    return { error: url.startsWith("blob:") ? "stream" : "blocked" };
  }
  if (!response.ok) {
    return { error: "http", status: response.status };
  }
  const blob = await response.blob();
  if (blob.size > maxBytes) {
    return { error: "too-large" };
  }
  const disposition = response.headers.get("content-disposition") ?? "";
  // Repeated rather than shared: this function runs in the page, alone.
  const match = /filename\*?=(?:UTF-8'')?"?(?<name>[^";]+)"?/iu.exec(
    disposition
  );
  // oxlint-disable-next-line promise/avoid-new -- FileReader only reports through events
  const dataUrl = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.addEventListener("load", () => resolve(reader.result as string));
    reader.addEventListener("error", () => reject(reader.error));
    reader.readAsDataURL(blob);
  });
  return {
    dataUrl,
    name: match?.groups?.name
      ? decodeURIComponent(match.groups.name)
      : undefined,
    type: blob.type,
  };
}

/** What was selected, and the language it is in when it is code. */
export interface SelectedText {
  text: string;
  /** Only for a selection inside a code block; prose stays plain text. */
  language?: string;
}

/**
 * The text selected in the frame that was clicked, with its line breaks, and
 * its language when it is code. Code is a selection in `<pre>`/`<code>`, or
 * in a code font: GitHub's file view has neither element, and selects in a
 * clear `<textarea>` over its lines.
 *
 * Its language, most reliable first: the block's classes where the page
 * marked it (Prism, highlight.js, GitHub's rendered Markdown); the file the
 * page shows, from its address (`/blob/main/src/link.rs`); a guess from the
 * text. The guess is weak on a few lines, so it comes last.
 *
 * Outside code nothing is guessed: prose taken for YAML or Markdown would
 * come out of the share page as a code listing.
 */
export async function readSelection(
  clickedIn: ClickedIn | undefined,
  fallback: string
): Promise<SelectedText> {
  // The menu's own `selectionText` collapses line breaks in Chrome, which
  // ruins code and poems alike, so the page is asked first.
  if (!clickedIn) {
    return { text: fallback };
  }
  let found: InPageSelection | undefined;
  try {
    const [injection] = await browser.scripting.executeScript({
      func: readSelectionInPage,
      target: { frameIds: [clickedIn.frameId], tabId: clickedIn.tabId },
    });
    found = injection?.result as InPageSelection | undefined;
  } catch {
    // A page the extension may not script: the menu's text is all there is.
  }
  if (!found?.text.trim()) {
    return { text: fallback };
  }
  if (!found.inCode) {
    return { text: found.text };
  }
  const named = found.classes
    .map((className) => languageFromClassName(className))
    .find((language) => language !== null);
  return {
    language:
      named ??
      fileLanguage(found.path) ??
      detectLanguage(found.text) ??
      undefined,
    text: found.text,
  };
}

/**
 * Extensions of addresses that are web pages, not files of code: a blog at
 * `/post.php` is not a PHP listing, whatever code it shows.
 */
const PAGE_EXTENSIONS = new Set([
  "asp",
  "aspx",
  "htm",
  "html",
  "jsp",
  "php",
  "shtml",
  "xhtml",
]);

/** The language of the file a page shows, judged by its address. */
function fileLanguage(path: string): string | null {
  const extension = /\.(?<ext>[^./]+)$/u.exec(path)?.groups?.ext?.toLowerCase();
  if (!extension || PAGE_EXTENSIONS.has(extension)) {
    return null;
  }
  return languageFromPath(path);
}

interface InPageSelection {
  text: string;
  inCode: boolean;
  /** The frame's own address path, which names the file a code view shows. */
  path: string;
  /** Class names around the code block, nearest first, `data-lang` included. */
  classes: string[];
}

/** Runs inside the page, alone: nothing from this module is in scope. */
// oxlint-disable-next-line complexity -- injected on its own, so it cannot be split into helpers here
function readSelectionInPage(): InPageSelection {
  const path = decodeURIComponent(location.pathname);
  const codeFont = /mono|courier|consol|menlo|monaco/iu;
  // A selection inside a text field is the field's, not the document's.
  // GitHub's file view is one: dragging over its lines selects in a clear
  // <textarea> laid over them, in the same code font.
  const field = document.activeElement;
  if (
    (field instanceof HTMLTextAreaElement ||
      field instanceof HTMLInputElement) &&
    field.selectionStart !== null &&
    field.selectionEnd !== null &&
    field.selectionStart !== field.selectionEnd
  ) {
    return {
      classes: [],
      inCode: codeFont.test(getComputedStyle(field).fontFamily),
      path,
      text: field.value.slice(field.selectionStart, field.selectionEnd),
    };
  }
  const selection = globalThis.getSelection();
  const text = selection?.toString() ?? "";
  if (!selection || selection.rangeCount === 0) {
    return { classes: [], inCode: false, path, text };
  }
  const range = selection.getRangeAt(0);
  // The first text actually selected: a range's own ends can sit on a
  // layout wrapper in another font than what it selects.
  const common = range.commonAncestorContainer;
  let first: Node | null = common.nodeType === Node.TEXT_NODE ? common : null;
  const walker = document.createTreeWalker(common, NodeFilter.SHOW_TEXT);
  while (!first) {
    const next = walker.nextNode();
    if (!next) {
      break;
    }
    if (range.intersectsNode(next) && next.textContent?.trim()) {
      first = next;
    }
  }
  const inside = first?.parentElement ?? null;
  const block =
    (common instanceof Element ? common : common.parentElement)?.closest(
      "pre, code"
    ) ?? inside?.closest("pre, code");
  if (!block) {
    // A code view drawn without <pre> still uses a code font.
    const font = inside ? getComputedStyle(inside).fontFamily : "";
    return { classes: [], inCode: codeFont.test(font), path, text };
  }
  // Prism and highlight.js mark the <code> or the <pre>; GitHub marks a
  // wrapper around the <pre>.
  const pre = block.closest("pre") ?? block;
  const around = [
    block.matches("code") ? block : pre.querySelector("code"),
    pre,
    pre.parentElement,
    pre.parentElement?.parentElement,
  ];
  const classes: string[] = [];
  for (const each of around) {
    if (each) {
      classes.push(each.getAttribute("class") ?? "");
    }
    // `data-lang` is HTML's; an SVG or MathML element has no `dataset`.
    const named =
      each instanceof HTMLElement
        ? (each.dataset.lang ?? each.dataset.language)
        : undefined;
    if (named) {
      classes.push(`language-${named}`);
    }
  }
  return { classes, inCode: true, path, text };
}

const FILENAME_IN_DISPOSITION = /filename\*?=(?:UTF-8'')?"?(?<name>[^";]+)"?/iu;

const EXTENSIONS: Record<string, string> = {
  "audio/mpeg": "mp3",
  "audio/ogg": "ogg",
  "audio/wav": "wav",
  "image/avif": "avif",
  "image/gif": "gif",
  "image/jpeg": "jpg",
  "image/png": "png",
  "image/svg+xml": "svg",
  "image/webp": "webp",
  "video/mp4": "mp4",
  "video/webm": "webm",
};

/**
 * A name for a downloaded file: the server's, else the last part of the
 * path, else one made from its type.
 */
export function fileName(url: string, headers: Headers, type: string): string {
  const disposition = FILENAME_IN_DISPOSITION.exec(
    headers.get("content-disposition") ?? ""
  );
  if (disposition?.groups?.name) {
    return safeDecode(disposition.groups.name);
  }
  try {
    const { pathname, protocol } = new URL(url);
    const last = pathname.split("/").findLast(Boolean);
    if (last && protocol !== "data:" && protocol !== "blob:") {
      const name = safeDecode(last);
      return name.includes(".") || !EXTENSIONS[type]
        ? name
        : `${name}.${EXTENSIONS[type]}`;
    }
  } catch {
    // Not a URL with a path; fall through to a made-up name.
  }
  const [kind] = type.split("/");
  return `${kind || "file"}.${EXTENSIONS[type] ?? "bin"}`;
}

function safeDecode(text: string): string {
  try {
    return decodeURIComponent(text);
  } catch {
    return text;
  }
}
