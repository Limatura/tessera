import { Button } from "@tessera/ui/components/button";
import { Settings2 } from "lucide-react";
import { useState } from "react";

import { getLocale } from "@/lib/locale";
import { minutesUntil } from "@/lib/upload";
import * as m from "@/paraglide/messages";
import { useRecentSharesStore } from "@/stores/recent-shares";
import type { RecentShare } from "@/stores/recent-shares";

import { ManageShareDialog } from "./manage-share-dialog";
import type { ManagedShare } from "./manage-share-dialog";
import { expiresAtFrom } from "./share-settings-panel";

/** 收起來時只列這幾筆。清單是給「剛剛傳的那個」用的,長了反而找不到。 */
const COLLAPSED_COUNT = 3;

/**
 * 首頁上傳卡片底下:這台瀏覽器匿名上傳過、還記得管理 token 的分享(#364)。
 *
 * 關掉結果畫面之後,這裡是匿名上傳者唯一回得來改設定或刪掉分享的地方。沒有紀錄
 * 就整塊不畫,第一次來的人不會看到一個空清單。
 */
export function RecentShares() {
  const shares = useRecentSharesStore((state) => state.shares);
  const update = useRecentSharesStore((state) => state.update);
  const forget = useRecentSharesStore((state) => state.forget);
  const [expanded, setExpanded] = useState(false);
  const [managing, setManaging] = useState<ManagedShare | null>(null);
  const [open, setOpen] = useState(false);

  if (shares.length === 0) {
    return null;
  }

  const visible = expanded ? shares : shares.slice(0, COLLAPSED_COUNT);

  const handleManage = (share: RecentShare) => {
    setManaging(toManaged(share));
    setOpen(true);
  };

  return (
    <section
      aria-labelledby="recent-shares-heading"
      className="mt-4 rounded-xl border border-border/60 bg-card/60 p-3 backdrop-blur-2xl"
    >
      <div className="flex items-baseline justify-between gap-2 px-1 pb-2">
        <h2 className="font-medium text-sm" id="recent-shares-heading">
          {m.recent_shares_title()}
        </h2>
        <p className="text-muted-foreground text-xs">
          {m.recent_shares_hint()}
        </p>
      </div>

      <ul className="flex flex-col">
        {visible.map((share) => (
          <li key={share.code}>
            <button
              className="flex w-full items-center gap-3 rounded-lg px-2 py-2 text-left transition-colors hover:bg-muted/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              onClick={() => handleManage(share)}
              type="button"
            >
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="truncate text-sm">{share.fileName}</span>
                <span className="text-muted-foreground text-xs">
                  <span className="font-mono">{share.code}</span>
                  {" · "}
                  {expiryLabel(share.expiresAt)}
                </span>
              </span>
              <Settings2
                aria-hidden
                className="size-4 shrink-0 text-muted-foreground"
              />
            </button>
          </li>
        ))}
      </ul>

      {shares.length > COLLAPSED_COUNT && (
        <Button
          className="mt-1 w-full text-muted-foreground"
          onClick={() => setExpanded((value) => !value)}
          size="sm"
          type="button"
          variant="ghost"
        >
          {expanded
            ? m.recent_shares_show_less()
            : m.recent_shares_show_all({ count: shares.length })}
        </Button>
      )}

      <ManageShareDialog
        onDeleted={forget}
        onForget={forget}
        onOpenChange={setOpen}
        onSettingsChange={(code, snapshot) =>
          update(code, {
            expiresAt: expiresAtFrom(snapshot.expiresInMinutes),
            hasPassword: snapshot.hasPassword,
            usesRemaining: snapshot.burnAfterReading ? 1 : null,
          })
        }
        open={open}
        share={managing}
        verify
      />
    </section>
  );
}

function toManaged(share: RecentShare): ManagedShare {
  return {
    code: share.code,
    initial: {
      burnAfterReading: share.usesRemaining === 1,
      expiresInMinutes: minutesUntil(share.expiresAt),
      hasPassword: share.hasPassword,
    },
    managementToken: share.managementToken,
    shortUrl: share.shortUrl,
    title: share.fileName,
  };
}

const MINUTE_MS = 60_000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;

/** 「3 天後到期」這種相對說法。清單是快照,打開管理時才會跟 server 對一次。 */
function expiryLabel(expiresAt: string | null): string {
  if (!expiresAt) {
    return m.recent_shares_never_expires();
  }
  const remaining = new Date(expiresAt).getTime() - Date.now();
  if (remaining <= 0) {
    return m.recent_shares_expired();
  }
  const format = new Intl.RelativeTimeFormat(getLocale(), { numeric: "auto" });
  if (remaining >= DAY_MS) {
    return m.recent_shares_expires({
      when: format.format(Math.round(remaining / DAY_MS), "day"),
    });
  }
  if (remaining >= HOUR_MS) {
    return m.recent_shares_expires({
      when: format.format(Math.round(remaining / HOUR_MS), "hour"),
    });
  }
  return m.recent_shares_expires({
    when: format.format(
      Math.max(1, Math.round(remaining / MINUTE_MS)),
      "minute"
    ),
  });
}
