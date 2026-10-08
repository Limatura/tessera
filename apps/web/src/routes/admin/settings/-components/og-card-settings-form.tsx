import { html } from "@codemirror/lang-html";
import { zodResolver } from "@hookform/resolvers/zod";
import type { components } from "@tessera/api/schema";
import { Button } from "@tessera/ui/components/button";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@tessera/ui/components/field";
import { cn } from "@tessera/ui/lib/utils";
import type { ReactCodeMirrorRef } from "@uiw/react-codemirror";
import { CircleAlert, CircleCheck, LoaderCircle } from "lucide-react";
import { useRef } from "react";
import { Controller, useWatch } from "react-hook-form";

import { CodeEditor } from "@/components/code-editor";
import { useLayoutCheck } from "@/hooks/admin/use-og-card";
import { readSetting } from "@/hooks/admin/use-settings";
import { useSettingsForm } from "@/hooks/admin/use-settings-form";
import type { SettingsKeyMap } from "@/hooks/admin/use-settings-form";
import {
  LAYOUT_MAX_BYTES,
  PLACEHOLDERS,
  STARTER_LAYOUT,
  asSiteLanguage,
  layoutBytes,
} from "@/lib/admin/og-card";
import type { SiteLanguage } from "@/lib/admin/og-card";
import { ogCardSettingsSchema } from "@/lib/admin/settings-schema";
import type { OgCardSettingsValues } from "@/lib/admin/settings-schema";
import { asString } from "@/lib/admin/settings-values";
import * as m from "@/paraglide/messages";

import { OgCardBuiltInNotice, OgCardPreview } from "./og-card-preview";
import { SettingsFormFooter } from "./settings-form-footer";
import { SettingsLoader } from "./settings-loader";

type Settings = components["schemas"]["SettingsResponse"];

const KEYS = {
  layout: "og.layout",
} satisfies SettingsKeyMap<OgCardSettingsValues>;

// 模組層級建一次:每次 render 都給新的 extension 陣列,CodeMirror 會整個重新設定。
const EXTENSIONS = [html()];

/**
 * `og.layout`:分享連結在聊天軟體裡那張預覽圖的版面。
 *
 * 編輯器、即時預覽、server 驗證三件事在同一頁,因為它們回答的是同一個問題:
 * 存下去之後,別人看到的卡片長怎樣。
 */
export function OgCardSettingsForm() {
  return (
    <SettingsLoader>
      {(settings) => (
        <OgCardFields
          defaultValues={initialValues(settings)}
          language={asSiteLanguage(readSetting(settings, "site.language"))}
        />
      )}
    </SettingsLoader>
  );
}

function initialValues(settings: Settings): OgCardSettingsValues {
  return { layout: asString(readSetting(settings, KEYS.layout), "") };
}

function OgCardFields({
  defaultValues,
  language,
}: {
  defaultValues: OgCardSettingsValues;
  language: SiteLanguage;
}) {
  const form = useSettingsForm<OgCardSettingsValues>({
    defaultValues,
    keys: KEYS,
    resolver: zodResolver(ogCardSettingsSchema),
  });
  const layout = useWatch({ control: form.control, name: "layout" });
  const isBuiltIn = layout.trim() === "";

  const editorRef = useRef<ReactCodeMirrorRef>(null);

  const handleUseStarter = () =>
    form.setValue("layout", STARTER_LAYOUT, { shouldDirty: true });
  const handleClear = () => form.setValue("layout", "", { shouldDirty: true });

  // 插在游標處而不是接在最後:佔位符幾乎都是要放進某個 div 裡面。編輯器自己的
  // onChange 會接手,所以表單一樣會標成 dirty。
  const handleInsert = (name: string) => {
    const view = editorRef.current?.view;
    if (!view) {
      return;
    }
    view.dispatch(view.state.replaceSelection(`{{${name}}}`));
    view.focus();
  };

  return (
    <form onSubmit={form.submit}>
      <FieldGroup>
        {/*
          編輯器在上、預覽在下。設定頁的內容欄限寬,左右並排的話編輯器只剩兩百多
          px,一行 HTML 都放不下;預覽照 1200×630 縮放,疊在下面剛好是整欄寬。
        */}
        <div className="grid gap-6">
          <Controller
            control={form.control}
            name="layout"
            render={({ field }) => (
              <Field className="min-w-0">
                <div className="flex min-h-8 items-center justify-between gap-2">
                  <FieldLabel id="og-layout-label">
                    {m.settings_og_layout_label()}
                  </FieldLabel>
                  {!isBuiltIn && form.canWrite && (
                    <Button
                      onClick={handleClear}
                      size="sm"
                      type="button"
                      variant="ghost"
                    >
                      {m.settings_og_clear()}
                    </Button>
                  )}
                </div>
                <CodeEditor
                  editable={form.canWrite}
                  extensions={EXTENSIONS}
                  height="440px"
                  labelledBy="og-layout-label"
                  onBlur={field.onBlur}
                  onChange={field.onChange}
                  ref={editorRef}
                  value={field.value}
                />
                <LayoutStatus canWrite={form.canWrite} layout={layout} />
                <PlaceholderChips
                  disabled={!form.canWrite}
                  onInsert={handleInsert}
                />
                <FieldDescription>
                  {m.settings_og_layout_description()}
                </FieldDescription>
              </Field>
            )}
          />

          {isBuiltIn ? (
            <OgCardBuiltInNotice
              canWrite={form.canWrite}
              onUseStarter={handleUseStarter}
            />
          ) : (
            <OgCardPreview language={language} layout={layout} />
          )}
        </div>

        <SettingsFormFooter form={form} />
      </FieldGroup>
    </form>
  );
}

function PlaceholderChips({
  disabled,
  onInsert,
}: {
  disabled: boolean;
  onInsert: (name: string) => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-1.5 text-xs">
      <span className="text-muted-foreground">
        {m.settings_og_placeholders()}
      </span>
      {PLACEHOLDERS.map((name) => (
        <button
          className="rounded-sm border bg-muted px-1.5 py-0.5 font-mono transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50"
          disabled={disabled}
          key={name}
          onClick={() => onInsert(name)}
          type="button"
        >
          {`{{${name}}}`}
        </button>
      ))}
    </div>
  );
}

/**
 * 編輯器底下那一行:用了多少位元組、server 收不收。
 *
 * 沒有寫入權限的人存不了,也打不了驗證那支(它要 `settings:write`),所以只顯示大小。
 */
function LayoutStatus({
  layout,
  canWrite,
}: {
  layout: string;
  canWrite: boolean;
}) {
  const check = useLayoutCheck(layout, canWrite);
  const used = layoutBytes(layout);
  const isEmpty = layout.trim() === "";

  return (
    <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-1 text-xs">
      <div aria-live="polite" className="min-w-0">
        {canWrite && !isEmpty && <CheckResult {...check} />}
      </div>
      <p
        className={cn(
          "shrink-0 tabular-nums",
          used > LAYOUT_MAX_BYTES ? "text-destructive" : "text-muted-foreground"
        )}
      >
        {m.settings_og_bytes({ max: LAYOUT_MAX_BYTES, used })}
      </p>
    </div>
  );
}

function CheckResult({
  isChecking,
  refusal,
}: {
  isChecking: boolean;
  refusal: string | null;
}) {
  if (isChecking) {
    return (
      <span className="flex items-center gap-1.5 text-muted-foreground">
        <LoaderCircle aria-hidden="true" className="size-3.5 animate-spin" />
        {m.settings_og_check_pending()}
      </span>
    );
  }
  if (refusal) {
    return (
      <span className="flex items-start gap-1.5 text-destructive">
        <CircleAlert aria-hidden="true" className="mt-0.5 size-3.5 shrink-0" />
        {refusal}
      </span>
    );
  }
  return (
    <span className="flex items-center gap-1.5 text-muted-foreground">
      <CircleCheck aria-hidden="true" className="size-3.5 text-primary" />
      {m.settings_og_check_ok()}
    </span>
  );
}
