/**
 * A split file fetched piece by piece from the stores holding it, so its
 * bytes never cross this server. See `docs/retrieval.md`, "Joining the pieces
 * in the browser".
 *
 * `GET /s/{code}/segments` describes the pieces and costs no retrieval.
 * `GET /s/{code}/segments/{i}` answers `307` to the store, with a URL that
 * lives fifteen seconds, so each piece is asked for only when it is wanted.
 */

export interface SegmentLayout {
  size_bytes: number;
  segments: { offset: number; length: number }[];
}

/**
 * The layout, or `null` when this share cannot be fetched this way (`404`) and
 * `/download` is the only road. `file` narrows a share of several files to one.
 */
export async function fetchSegmentLayout(
  code: string,
  file?: number,
  signal?: AbortSignal
): Promise<SegmentLayout | null> {
  const response = await fetch(segmentsPath(code, undefined, file), {
    credentials: "include",
    signal,
  });
  if (!response.ok) {
    return null;
  }
  return (await response.json()) as SegmentLayout;
}

/**
 * The first piece's response, fetched up front. A bucket without CORS fails
 * here, before anything has been written, so the caller can still fall back
 * to `/download`.
 */
export function fetchSegment(
  code: string,
  index: number,
  file?: number,
  signal?: AbortSignal
): Promise<Response> {
  return fetch(segmentsPath(code, index, file), { signal }).then((response) => {
    if (!response.ok || !response.body) {
      throw new Error(`segment ${index} failed: ${response.status}`);
    }
    return response;
  });
}

/**
 * Every piece, in order, as one stream. `first` is the already-fetched
 * response for piece 0. Each piece must be exactly as long as the layout says:
 * a store answering short would otherwise shift every later byte.
 */
export function joinSegments(
  code: string,
  layout: SegmentLayout,
  first: Response,
  file?: number,
  signal?: AbortSignal
): ReadableStream<Uint8Array> {
  let index = 0;
  let current: ReadableStreamDefaultReader<Uint8Array> | undefined =
    first.body?.getReader();
  let received = 0;

  return new ReadableStream<Uint8Array>({
    cancel: async () => {
      await current?.cancel();
    },
    pull: async (controller) => {
      while (index < layout.segments.length) {
        const expected = layout.segments[index]?.length ?? 0;
        if (!current) {
          // oxlint-disable-next-line no-await-in-loop -- pieces are joined in order
          const response = await fetchSegment(code, index, file, signal);
          current = response.body?.getReader();
          received = 0;
        }
        // oxlint-disable-next-line no-await-in-loop -- one read per pull
        const { done, value } = (await current?.read()) ?? { done: true };
        if (!done && value) {
          received += value.length;
          if (received > expected) {
            throw new Error(`segment ${index} ran long`);
          }
          controller.enqueue(value);
          return;
        }
        if (received !== expected) {
          throw new Error(
            `segment ${index} ended at ${received} of ${expected}`
          );
        }
        current = undefined;
        index += 1;
      }
      controller.close();
    },
  });
}

function segmentsPath(code: string, index?: number, file?: number) {
  const base = `/s/${encodeURIComponent(code)}/segments`;
  const path = index === undefined ? base : `${base}/${index}`;
  return file === undefined ? path : `${path}?file=${file}`;
}
