import { Button } from "@tessera/ui/components/button";
import { Input } from "@tessera/ui/components/input";
import { Label } from "@tessera/ui/components/label";
import { Tabs, TabsList, TabsTrigger } from "@tessera/ui/components/tabs";
import { cn } from "@tessera/ui/lib/utils";
import { useState } from "react";
import type * as React from "react";

import { useCardPreviews } from "@/hooks/admin/use-og-card";
import type { CardImage } from "@/hooks/admin/use-og-card";
import { CARD_STATES, SAMPLE_TITLE } from "@/lib/admin/og-card";
import type { CardState, SiteLanguage } from "@/lib/admin/og-card";
import * as m from "@/paraglide/messages";

interface OgCardPreviewProps {
  layout: string;
  language: SiteLanguage;
}

/**
 * 版面的即時預覽面板:狀態切換、卡片、範例標題,框成一塊。
 *
 * 四種狀態每次都一起畫(見 `useCardPreviews`),所以切換是立即的。範例標題只活在
 * 這個元件裡,不會被存 —— 它是拿來試版面的,不是設定。
 */
export function OgCardPreview({ layout, language }: OgCardPreviewProps) {
  const [title, setTitle] = useState(SAMPLE_TITLE);
  const [selected, setSelected] = useState<CardState>("open");
  const { engine, images, isStale } = useCardPreviews({
    language,
    layout,
    title,
  });

  const handleStateChange = (value: unknown) => {
    const next = CARD_STATES.find((state) => state === value);
    if (next) {
      setSelected(next);
    }
  };

  if (engine.status === "failed") {
    return (
      <PreviewPanel>
        <p className="text-destructive text-sm" role="alert">
          {m.settings_og_engine_failed({ reason: engine.error })}
        </p>
      </PreviewPanel>
    );
  }

  return (
    <PreviewPanel
      action={
        <Tabs onValueChange={handleStateChange} value={selected}>
          <TabsList aria-label={m.settings_og_states_label()}>
            {CARD_STATES.map((state) => (
              <TabsTrigger key={state} value={state}>
                {stateLabel(state)}
                {hasError(images[state]) && (
                  <span
                    aria-hidden="true"
                    className="size-1.5 rounded-full bg-destructive"
                  />
                )}
              </TabsTrigger>
            ))}
          </TabsList>
        </Tabs>
      }
    >
      <CardFrame
        busy={engine.status === "loading" || isStale}
        image={images[selected]}
        label={stateLabel(selected)}
        loadingText={
          engine.status === "loading"
            ? m.settings_og_engine_loading()
            : m.settings_og_drawing()
        }
      />

      {selected === "locked" && (
        <p className="text-muted-foreground text-xs/relaxed">
          {m.settings_og_locked_note()}
        </p>
      )}

      <div className="flex flex-col gap-1.5">
        <Label htmlFor="og-sample-title">
          {m.settings_og_sample_title_label()}
        </Label>
        <Input
          aria-describedby="og-sample-title-hint"
          id="og-sample-title"
          onChange={(event) => setTitle(event.target.value)}
          value={title}
        />
        <p
          className="text-muted-foreground text-xs/relaxed"
          id="og-sample-title-hint"
        >
          {m.settings_og_sample_title_description()}
        </p>
      </div>

      <p className="border-t pt-3 text-muted-foreground text-xs/relaxed">
        {m.settings_og_preview_hint()}
      </p>
    </PreviewPanel>
  );
}

/**
 * 沒有自訂版面時,預覽的位置放這個,而不是空著 —— 開始的按鈕就在看預覽的地方。
 */
export function OgCardBuiltInNotice({
  canWrite,
  onUseStarter,
}: {
  canWrite: boolean;
  onUseStarter: () => void;
}) {
  return (
    <PreviewPanel>
      <div className="flex aspect-1200/630 w-full flex-col items-center justify-center gap-3 rounded-md border border-dashed p-6 text-center">
        <p className="max-w-sm text-muted-foreground text-sm">
          {m.settings_og_builtin_note()}
        </p>
        {canWrite && (
          <Button onClick={onUseStarter} size="sm" type="button">
            {m.settings_og_use_starter()}
          </Button>
        )}
      </div>
    </PreviewPanel>
  );
}

/** 寬螢幕時是編輯器右邊那一欄;窄螢幕疊在編輯器下面,寬度封頂免得卡片撐太大。 */
function PreviewPanel({
  action,
  children,
}: {
  action?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section
      aria-labelledby="og-preview-heading"
      className="flex w-full max-w-2xl flex-col gap-3 self-start rounded-lg border bg-muted/30 p-3 2xl:max-w-none"
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 className="font-medium text-sm" id="og-preview-heading">
          {m.settings_og_preview_label()}
        </h3>
        {action}
      </div>
      {children}
    </section>
  );
}

function CardFrame({
  image,
  label,
  busy,
  loadingText,
}: {
  image: CardImage | undefined;
  label: string;
  busy: boolean;
  loadingText: string;
}) {
  return (
    <figure className="relative aspect-1200/630 w-full overflow-hidden rounded-md border bg-background">
      {image && "url" in image && (
        <img
          alt={m.settings_og_preview_alt({ state: label })}
          className={cn(
            "size-full object-contain transition-opacity",
            busy && "opacity-60"
          )}
          height={630}
          src={image.url}
          width={1200}
        />
      )}
      {image && "error" in image && (
        <p
          className="absolute inset-0 flex items-center justify-center p-6 text-center text-destructive text-sm"
          role="alert"
        >
          {m.settings_og_render_failed({ reason: image.error })}
        </p>
      )}
      {!image && (
        <p
          aria-live="polite"
          className="absolute inset-0 flex items-center justify-center text-muted-foreground text-sm"
        >
          {loadingText}
        </p>
      )}
    </figure>
  );
}

function hasError(image: CardImage | undefined): boolean {
  return image !== undefined && "error" in image;
}

function stateLabel(state: CardState): string {
  switch (state) {
    case "open": {
      return m.settings_og_state_open();
    }
    case "locked": {
      return m.settings_og_state_locked();
    }
    case "sealed": {
      return m.settings_og_state_sealed();
    }
    case "gone": {
      return m.settings_og_state_gone();
    }
    default: {
      throw new Error(`unknown card state: ${state satisfies never}`);
    }
  }
}
