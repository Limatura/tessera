import { useQuery } from "@tanstack/react-query";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";
import { useEffect, useState } from "react";
import { useDebounce } from "use-debounce";

import { CARD_STATES, fill, sampleValues } from "@/lib/admin/og-card";
import type { CardState, SiteLanguage } from "@/lib/admin/og-card";
import { loadCardRenderer, renderCard } from "@/lib/admin/og-renderer";
import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

import { adminKeys } from "./keys";

type Changes = components["schemas"]["UpdateSettingsRequest"]["changes"];

const RENDER_DEBOUNCE_MS = 250;
const CHECK_DEBOUNCE_MS = 600;

export type CardImage = { url: string } | { error: string };

export type EngineState =
  | { status: "loading" }
  | { status: "ready" }
  | { error: string; status: "failed" };

/**
 * Renders the card for all four states together.
 *
 * All four instead of just the selected one because layout bugs on locked
 * shares are the hardest to notice: testing with your own files looks fine.
 * With all four thumbnails in view, a gap in the locked one is obvious.
 */
export function useCardPreviews({
  layout,
  language,
  title,
}: {
  layout: string;
  language: SiteLanguage;
  title: string;
}) {
  const [debouncedLayout] = useDebounce(layout, RENDER_DEBOUNCE_MS);
  const [debouncedTitle] = useDebounce(title, RENDER_DEBOUNCE_MS);
  const [engine, setEngine] = useState<EngineState>({ status: "loading" });
  const [images, setImages] = useState<Partial<Record<CardState, CardImage>>>(
    {}
  );

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      try {
        await loadCardRenderer();
        if (!cancelled) {
          setEngine({ status: "ready" });
        }
      } catch (error) {
        if (!cancelled) {
          setEngine({ error: describe(error), status: "failed" });
        }
      }
    };
    load();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    // An empty layout needs no render: the caller unmounts this preview as soon
    // as the layout is cleared, so this only happens before the debounce catches up.
    if (engine.status !== "ready" || debouncedLayout.trim() === "") {
      return;
    }
    let cancelled = false;

    const drawAll = async () => {
      const drawn = await Promise.all(
        CARD_STATES.map(async (state): Promise<[CardState, CardImage]> => {
          const markup = fill(
            debouncedLayout,
            sampleValues(state, language, debouncedTitle)
          );
          try {
            const blob = await renderCard(markup);
            return [state, { url: URL.createObjectURL(blob) }];
          } catch (error) {
            return [state, { error: describe(error) }];
          }
        })
      );
      // A render cannot be cancelled, only discarded. Revoke the URLs first or
      // the blobs stay alive.
      if (cancelled) {
        revokeAll(Object.fromEntries(drawn));
        return;
      }
      setImages(Object.fromEntries(drawn));
    };
    drawAll();

    return () => {
      cancelled = true;
    };
  }, [engine.status, debouncedLayout, debouncedTitle, language]);

  // Revoke the previous batch's blob URLs when a new batch arrives (or the page is left).
  useEffect(() => () => revokeAll(images), [images]);

  return {
    engine,
    images,
    isStale: debouncedLayout !== layout || debouncedTitle !== title,
  };
}

/**
 * Asks the server whether this layout could be saved, without saving it.
 *
 * Only the server's rules are authoritative (`og::layout::check`, and `tw`
 * needs takumi itself), so instead of copying them here this calls
 * `POST /admin/settings/preview`, which runs the same validation as `PUT`
 * without writing. `null` means it would be accepted.
 */
export function useLayoutCheck(layout: string, enabled: boolean) {
  const [debounced] = useDebounce(layout, CHECK_DEBOUNCE_MS);
  const query = useQuery({
    enabled: enabled && debounced.trim() !== "",
    queryFn: async (): Promise<string | null> => {
      const changes: Record<string, unknown> = { "og.layout": debounced };
      const { error } = await api.POST("/api/v1/admin/settings/preview", {
        // As in useUpdateSettings: the cast only works around
        // openapi-typescript's type for arbitrary JSON.
        body: { changes: changes as Changes },
      });
      return error
        ? apiErrorMessage(error, m.settings_form_save_failed())
        : null;
    },
    queryKey: adminKeys.ogLayoutCheck(debounced),
    retry: false,
    staleTime: Number.POSITIVE_INFINITY,
  });

  return {
    isChecking: query.isFetching || debounced !== layout,
    refusal: query.data ?? null,
  };
}

function revokeAll(images: Partial<Record<CardState, CardImage>>) {
  for (const image of Object.values(images)) {
    if (image && "url" in image) {
      URL.revokeObjectURL(image.url);
    }
  }
}

function describe(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
