import type { components } from "@tessera/api/schema";
import type { ShareKey } from "@tessera/crypto";
import { openDetails, openName, openStream, plainSize } from "@tessera/crypto";

import { buildTransferUrl } from "@/lib/api/share";

import { fetchSegment, fetchSegmentLayout, joinSegments } from "./segments";

type PublicShare = components["schemas"]["PublicShareResponse"];

/** What the page can say about an encrypted share once it has the key. */
export interface OpenedShare {
  title?: string;
  language?: string;
  files: OpenedFile[];
}

export interface OpenedFile {
  /** Position in the share, for `?file=N`. */
  index: number;
  /** As sealed: may hold folders, `/`-separated. */
  path: string;
  type?: string;
  /** The sealed length, which the download sends and `openStream` needs. */
  sealedBytes: number;
  plainBytes: number;
}

/**
 * Opens the share's sealed title and file names. Throws when the key is
 * wrong: every sealed field is authenticated, so a wrong key cannot produce a
 * wrong name, only a failure.
 */
export function openShare(key: ShareKey, data: PublicShare): OpenedShare {
  const details = data.sealed_details
    ? openDetails(key, data.sealed_details)
    : {};
  return {
    files: data.files.map((file, index) => {
      const name = file.sealed_name
        ? openName(key, file.sealed_name)
        : { name: `file-${index + 1}` };
      return {
        index,
        path: safePath(name.name) || `file-${index + 1}`,
        plainBytes: plainSize(file.size_bytes),
        sealedBytes: file.size_bytes,
        type: name.type,
      };
    }),
    language: details.language,
    title: details.title,
  };
}

/**
 * Keeps only the ordinary parts of a sealed path, so a name cannot place a
 * file outside the folder it is saved into (`docs/encryption.md`).
 */
export function safePath(path: string): string {
  return path
    .split(/[/\\]/u)
    .filter((part) => part !== "" && part !== "." && part !== "..")
    .join("/");
}

export function baseName(path: string): string {
  return path.split("/").at(-1) ?? path;
}

/**
 * One file's plain bytes as they arrive. A share whose pieces the server
 * lets the browser fetch from the stores, split across more than one, is
 * joined from there; anything else comes from `/download?file=N`.
 *
 * `ticket` is asked for only when `/download` is the road, since getting one
 * spends a retrieval.
 */
export async function openedFileStream(
  code: string,
  key: ShareKey,
  file: OpenedFile,
  ticket: () => Promise<string>
): Promise<ReadableStream<Uint8Array>> {
  const sealed = await sealedStream(code, file, ticket);
  return sealed.pipeThrough(openStream(key, file.sealedBytes));
}

async function sealedStream(
  code: string,
  file: OpenedFile,
  ticket: () => Promise<string>
): Promise<ReadableStream<Uint8Array>> {
  const layout = await fetchSegmentLayout(code, file.index).catch(() => null);
  if (layout && layout.segments.length > 1) {
    try {
      const first = await fetchSegment(code, 0, file.index);
      return joinSegments(code, layout, first, file.index);
    } catch {
      // A bucket without CORS: the server still serves it
    }
  }
  const url = `${buildTransferUrl(code, "download", await ticket())}&file=${file.index}`;
  const response = await fetch(url);
  if (!response.ok || !response.body) {
    throw new Error(`download failed: ${response.status}`);
  }
  return response.body;
}

/** A whole file opened into memory, for a preview or a browser with no stream saving. */
export async function openedFileBlob(
  code: string,
  key: ShareKey,
  file: OpenedFile,
  ticket: () => Promise<string>
): Promise<Blob> {
  const stream = await openedFileStream(code, key, file, ticket);
  const bytes = await new Response(stream).arrayBuffer();
  return new Blob([bytes], file.type ? { type: file.type } : undefined);
}
