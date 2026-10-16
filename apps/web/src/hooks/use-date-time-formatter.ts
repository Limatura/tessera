import { createDateTimeFormatter } from "@tessera/ui/lib/format";

import { getLocale } from "@/lib/locale";
import { useAppearanceStore } from "@/stores/appearance";

/**
 * A date/time formatter that follows the UI language and the time format in
 * preferences.
 *
 * The locale is read directly: the root remounts on a language change, so this
 * reruns. The time format subscribes to the store, or switching between 12- and
 * 24-hour clocks in preferences would not reach pages already open.
 */
export function useDateTimeFormatter() {
  const timeFormat = useAppearanceStore((state) => state.timeFormat);
  return createDateTimeFormatter(getLocale(), timeFormat);
}
