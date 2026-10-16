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

// Built once at module level: a new extensions array on each render makes
// CodeMirror reconfigure from scratch.
const EXTENSIONS = [html()];

/**
 * `og.layout`: the layout of the preview image a chat app shows for a share link.
 *
 * Editor, live preview and server validation share a page because they answer one
 * question: what the card will look like once saved.
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

  // Inserted at the cursor, not appended: a placeholder nearly always goes inside some
  // div. The editor's own onChange takes over, so the form is still marked dirty.
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
          Editor on top, preview below. The settings content column is width-limited, and
          side by side the editor would get barely 200px, too narrow for a line of HTML;
          the preview scales from 1200×630, and below the editor it spans the column.
        */}
        <div className="grid gap-6">
          <Controller
            control={form.control}
            name="layout"
            render={({ field }) => (
              <Field className="min-w-0 max-w-2xl gap-3 rounded-lg border bg-muted/30 p-3 2xl:max-w-none">
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
 * The line under the editor: bytes used, and whether the server accepts it.
 *
 * Users without write permission cannot save or call validation (it needs
 * `settings:write`), so they only see the size.
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
