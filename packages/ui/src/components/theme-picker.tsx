import { IconCheck } from "@tabler/icons-react";
import { cn } from "@tessera/ui/lib/utils";

/**
 * 外觀選擇器：主題卡片與強調色色票。
 *
 * 放在 @tessera/ui 而不是各自複製一份，因為前台的個人偏好和後台的站台預設
 * 選的是**同一組值**，長得不一樣只會讓人以為它們是兩件事。
 *
 * 只做「長什麼樣 + 選了誰」這兩件事，狀態留給呼叫端：個人偏好是選了立刻送出，
 * 後台是 react-hook-form 的欄位、要按儲存才生效。把那個差異包進來會逼出一個
 * 同時服務兩種流程的 prop 清單，而那正是這裡不該知道的事。
 *
 * 文案也由呼叫端給。
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
 * 一張卡片一個色盤，預覽塊自己帶 `data-theme`，所以顯示的是那個主題**真正的**
 * background / card / muted / border，不是另外挑一組相近的色階。
 *
 * 好處是加主題時這個元件不用改，而且預覽不可能跟實際套用的結果對不上。
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
 * 底是 `background`，上面疊一張 `card` 和三條 `muted` 當內容，邊框用 `border`。
 *
 * 三個一起看才分得出主題之間的差別 —— 只放背景色的話 graphite 和 slate 幾乎
 * 一模一樣。明暗跟著目前狀態走（`.dark` 在祖先上，屬性選擇器在子樹一樣生效）。
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
 * 強調色色票。
 *
 * 每顆自己帶 `data-accent` 並用 `bg-primary`，所以顯示的就是選下去會變成的那個
 * 顏色 —— 不需要另外維護一張「Tailwind 色階對照 oklch 值」的表，那張表遲早會
 * 跟 globals.css 漂移。
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
