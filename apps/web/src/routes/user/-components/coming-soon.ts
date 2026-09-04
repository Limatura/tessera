import { toast } from "@tessera/ui/components/toast";

import * as m from "@/paraglide/messages";

/** 後端端點還沒接上之前,按鈕按下去給的統一回饋。 */
export function notifyComingSoon() {
  toast.add({ title: m.user_coming_soon(), type: "info" });
}
