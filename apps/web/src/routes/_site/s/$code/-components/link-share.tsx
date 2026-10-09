import type { components } from "@tessera/api/schema";
import { Button } from "@tessera/ui/components/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import { ExternalLink, Link2 } from "lucide-react";
import { useEffect, useState } from "react";

import { useShareTicket } from "@/hooks/use-share";
import { buildTransferUrl } from "@/lib/api/share";
import * as m from "@/paraglide/messages";

import { ShareMeta } from "./share-meta";

type PublicShare = components["schemas"]["PublicShareResponse"];

/**
 * 連結分享只有在有密碼或有次數限制時才會看到這頁,其他情況伺服器直接 302。
 *
 * 不自動拿票券、也不讀 `/raw` 顯示目的地:兩者都會花掉一次提取,Discord 之類的
 * 爬蟲展開網址就會把一次性連結用掉。所以次數限制要等人按「前往」才花。
 * 有密碼的話人已經輸入過密碼,解鎖本身就是意圖,直接導走。
 */
export function LinkShare({
  code,
  data,
  password,
}: {
  code: string;
  data: PublicShare;
  password?: string;
}) {
  const [requested, setRequested] = useState(data.has_password);
  const ticket = useShareTicket(code, password, requested);

  useEffect(() => {
    if (ticket.data) {
      window.location.href = buildTransferUrl(
        code,
        "download",
        ticket.data.ticket
      );
    }
  }, [ticket.data, code]);

  const going = ticket.isFetching || ticket.isSuccess;

  // 失敗後 query 仍是 enabled,再 set 一次不會重抓,要明確 refetch
  const handleGo = () => {
    if (requested) {
      ticket.refetch();
      return;
    }
    setRequested(true);
  };

  return (
    <div className="w-full max-w-sm animate-in fade-in duration-200">
      <Card className="w-full border-border/60 bg-card/60 shadow-2xl shadow-black/5 backdrop-blur-2xl">
        <CardHeader className="items-center text-center">
          <div className="mb-2 flex size-10 items-center justify-center rounded-full bg-muted text-muted-foreground">
            <Link2 className="size-5" />
          </div>
          <CardTitle className="break-words font-medium text-xl tracking-tight">
            {data.title || m.share_link_title()}
          </CardTitle>
          <CardDescription>{m.share_link_description()}</CardDescription>
          <ShareMeta
            expiresAt={data.expires_at}
            usesRemaining={data.uses_remaining}
          />
        </CardHeader>
        <CardContent>
          <Button
            className="w-full"
            disabled={going}
            onClick={handleGo}
            type="button"
          >
            <ExternalLink />
            {going ? m.share_link_redirecting() : m.share_link_go()}
          </Button>
        </CardContent>
      </Card>
    </div>
  );
}
