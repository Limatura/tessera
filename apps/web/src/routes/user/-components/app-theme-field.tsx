import { IconCheck } from "@tabler/icons-react";
import { cn } from "@tessera/ui/lib/utils";

import * as m from "@/paraglide/messages";
import type { AppTheme } from "@/stores/appearance";
import { useAppearanceStore } from "@/stores/appearance";

/**
 * 整體視覺主題。目前只把選擇存起來 —— 真正換掉色票要等 theme token 那組工作,
 * 所以這裡不動 document。
 */
export function AppThemeField() {
  const appTheme = useAppearanceStore((state) => state.appTheme);
  const setAppTheme = useAppearanceStore((state) => state.setAppTheme);

  const options: {
    value: AppTheme;
    name: string;
    description: string;
    preview: string;
  }[] = [
    {
      description: m.user_prefs_app_theme_default_description(),
      name: m.user_prefs_app_theme_default_name(),
      preview: "from-slate-100 to-white",
      value: "default",
    },
    {
      description: m.user_prefs_app_theme_graphite_description(),
      name: m.user_prefs_app_theme_graphite_name(),
      preview: "from-slate-700 to-slate-500",
      value: "graphite",
    },
    {
      description: m.user_prefs_app_theme_sepia_description(),
      name: m.user_prefs_app_theme_sepia_name(),
      preview: "from-amber-100 to-orange-50",
      value: "sepia",
    },
  ];

  return (
    <div className="space-y-3">
      <div>
        <p className="font-medium text-sm">{m.user_prefs_app_theme_title()}</p>
        <p className="text-muted-foreground text-sm">
          {m.user_prefs_app_theme_description()}
        </p>
      </div>
      <div className="grid gap-3 sm:grid-cols-3">
        {options.map((option) => (
          <button
            aria-pressed={appTheme === option.value}
            className={cn(
              "rounded-lg border p-3 text-left transition-colors hover:bg-accent/40",
              appTheme === option.value && "border-primary ring-1 ring-primary"
            )}
            key={option.value}
            onClick={() => setAppTheme(option.value)}
            type="button"
          >
            <div
              className={cn(
                "mb-3 h-20 rounded-md border bg-gradient-to-br",
                option.preview
              )}
            />
            <span className="flex items-center gap-1 font-medium text-sm">
              {option.name}
              {appTheme === option.value ? (
                <IconCheck className="text-primary" size={14} />
              ) : null}
            </span>
            <span className="mt-0.5 block text-muted-foreground text-xs">
              {option.description}
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}
