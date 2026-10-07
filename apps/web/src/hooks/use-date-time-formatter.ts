import { createDateTimeFormatter } from "@/lib/format";
import { getLocale } from "@/lib/locale";
import { useAppearanceStore } from "@/stores/appearance";

/**
 * 跟著介面語言與偏好設定裡的時間格式走的日期時間格式器。
 *
 * locale 直接讀:換語言時 root 會 remount,這裡會重跑。時間格式要訂閱 store,
 * 不然在偏好設定改了 12/24 小時制,開著的頁面不會跟著變。
 */
export function useDateTimeFormatter() {
  const timeFormat = useAppearanceStore((state) => state.timeFormat);
  return createDateTimeFormatter(getLocale(), timeFormat);
}
