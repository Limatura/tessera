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
 * 四種狀態的卡片一起畫。
 *
 * 一起畫而不是只畫選中的那張,是因為 issue #226 講的:版面在鎖定分享上出錯最難
 * 發現 —— 拿自己的檔案測都正常。四張縮圖都在眼前,鎖定那張空掉一塊就看得到。
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
    // 空版面不用畫:呼叫端在版面清空的當下就不掛這個預覽了,這裡只會在去抖動
    // 還沒跟上的那一瞬間看到它。
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
      // 畫圖取消不了,只能丟掉結果。丟掉之前把網址放掉,不然 blob 會一直留著。
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

  // 換上新的一批(或離開頁面)時,把上一批的 blob 網址放掉。
  useEffect(() => () => revokeAll(images), [images]);

  return {
    engine,
    images,
    isStale: debouncedLayout !== layout || debouncedTitle !== title,
  };
}

/**
 * 問 server 這份版面存不存得進去,但不真的存。
 *
 * 規則只有 server 那份是準的(`og::layout::check`,`tw` 還要問 takumi 本身),
 * 所以不在前端抄一份,而是打 `POST /admin/settings/preview` —— 它跟 `PUT` 走同一條
 * 驗證,只是不寫入。回 `null` 代表會被接受。
 */
export function useLayoutCheck(layout: string, enabled: boolean) {
  const [debounced] = useDebounce(layout, CHECK_DEBOUNCE_MS);
  const query = useQuery({
    enabled: enabled && debounced.trim() !== "",
    queryFn: async (): Promise<string | null> => {
      const changes: Record<string, unknown> = { "og.layout": debounced };
      const { error } = await api.POST("/api/v1/admin/settings/preview", {
        // 同 useUpdateSettings:轉型只是繞開 openapi-typescript 對任意 JSON 的表示法。
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
