import { IconCheck } from "@tabler/icons-react";
import { cn } from "@tessera/ui/lib/utils";

/**
 * Appearance pickers: theme cards and accent swatches.
 *
 * In @tessera/ui instead of copied per app, because a user's preference and the
 * site default in the admin panel choose from the same values; looking
 * different would suggest they are different things.
 *
 * Covers only how it looks and what is selected; state stays with the caller.
 * A personal preference is sent as soon as it is picked, while in the panel it
 * is a react-hook-form field applied on Save. Absorbing that difference would
 * force a prop list serving both flows, which this component should not know
 * about.
 *
 * Labels come from the caller too.
 */

interface ThemeOption<T extends string> {
  value: T;
  name: string;
  description?: string;
}

interface ThemePickerProps<T extends string> {
  options: readonly ThemeOption<T>[];
  value: T;
  onSelect: (value: T) => void;
  disabled?: boolean;
  className?: string;
}

/**
 * One card per palette. The preview carries its own `data-theme`, so it shows
 * the theme's actual background / card / muted / border, not a similar set
 * picked separately.
 *
 * Adding a theme needs no change here, and the preview cannot disagree with
 * what is applied.
 */
export function ThemePicker<T extends string>({
  className,
  disabled,
  onSelect,
  options,
  value,
}: ThemePickerProps<T>) {
  return (
    <div
      className={cn("grid gap-3 sm:grid-cols-3 lg:grid-cols-4", className)}
      role="radiogroup"
    >
      {options.map((option) => (
        <button
          aria-checked={value === option.value}
          className={cn(
            "rounded-lg border p-3 text-left transition-colors hover:bg-accent/40",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
            "disabled:opacity-60",
            value === option.value && "border-primary ring-1 ring-primary"
          )}
          disabled={disabled}
          key={option.value}
          onClick={() => onSelect(option.value)}
          role="radio"
          type="button"
        >
          <ThemeSwatch theme={option.value} />
          <span className="flex items-center gap-1 font-medium text-sm">
            {option.name}
            {value === option.value ? (
              <IconCheck className="text-primary" size={14} />
            ) : null}
          </span>
          {option.description ? (
            <span className="mt-0.5 block text-muted-foreground text-xs">
              {option.description}
            </span>
          ) : null}
        </button>
      ))}
    </div>
  );
}

/**
 * `background` as the base, a `card` on top with three `muted` bars as content,
 * edged with `border`.
 *
 * All three are needed to tell themes apart: with the background alone,
 * graphite and slate look almost identical. Light or dark follows the current
 * state (`.dark` is on an ancestor, and attribute selectors apply in the
 * subtree).
 */
function ThemeSwatch({ theme }: { theme: string }) {
  return (
    <div
      className="mb-3 h-20 overflow-hidden rounded-md border bg-background p-2"
      data-theme={theme}
    >
      <div className="flex h-full flex-col gap-1.5 rounded border bg-card p-2">
        <div className="h-1.5 w-3/5 rounded-full bg-muted-foreground/70" />
        <div className="h-1.5 w-full rounded-full bg-muted" />
        <div className="h-1.5 w-4/5 rounded-full bg-muted" />
      </div>
    </div>
  );
}

interface AccentOption<T extends string> {
  value: T;
  label: string;
}

interface AccentPickerProps<T extends string> {
  options: readonly AccentOption<T>[];
  value: T;
  onSelect: (value: T) => void;
  disabled?: boolean;
  className?: string;
}

/**
 * Accent swatches.
 *
 * Each sets its own `data-accent` and uses `bg-primary`, so it shows the colour
 * picking it will produce. No separate table mapping Tailwind shades to oklch
 * values, which would drift from globals.css sooner or later.
 */
export function AccentPicker<T extends string>({
  className,
  disabled,
  onSelect,
  options,
  value,
}: AccentPickerProps<T>) {
  return (
    <div className={cn("flex flex-wrap gap-2", className)} role="radiogroup">
      {options.map((option) => (
        <button
          aria-checked={value === option.value}
          aria-label={option.label}
          className={cn(
            "flex size-7 items-center justify-center rounded-full bg-primary text-primary-foreground",
            "ring-offset-2 ring-offset-background transition-shadow",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
            "disabled:opacity-60",
            value === option.value && "ring-2 ring-foreground"
          )}
          data-accent={option.value}
          disabled={disabled}
          key={option.value}
          onClick={() => onSelect(option.value)}
          role="radio"
          title={option.label}
          type="button"
        >
          {value === option.value ? <IconCheck size={14} /> : null}
        </button>
      ))}
    </div>
  );
}
