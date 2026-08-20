import { Badge } from "@tessera/ui/components/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";

import { formatBytes } from "@/lib/format";

/**
 * Mirrors the server's `Capabilities` response (apps/server/src/api/capabilities.rs)
 * plus a couple of admin-only fields from `Config`. Static placeholder values
 * until GET /capabilities (and an admin-only config readout) are wired up.
 *
 * Everything here is set via TESSERA_* env vars / config file at process
 * startup — none of it is editable through this UI.
 */
const config = {
  accounts: true,
  anonymousUpload: true,
  maxBodyBytes: 104_857_600,
  registration: "open" as const,
  retainAfterDeathHours: 168,
  storageUrl: "data/blobs",
};

function registrationLabel(registration: typeof config.registration) {
  if (registration === "open") {
    return "開放註冊";
  }
  if (registration === "invite") {
    return "限邀請碼";
  }
  return "已關閉";
}

const rows: { label: string; value: string }[] = [
  { label: "儲存後端", value: config.storageUrl },
  { label: "單次上傳上限", value: formatBytes(config.maxBodyBytes) },
  { label: "訪客上傳", value: config.anonymousUpload ? "已開啟" : "已關閉" },
  { label: "帳號系統", value: config.accounts ? "已開啟" : "已關閉" },
  {
    label: "註冊政策",
    value: registrationLabel(config.registration),
  },
  {
    label: "過期分享保留",
    value: `${config.retainAfterDeathHours} 小時`,
  },
];

export function ConfigSummary() {
  return (
    <Card className="ring-foreground/10">
      <CardHeader>
        <CardTitle>啟動設定</CardTitle>
        <CardDescription>
          由環境變數或設定檔在啟動時決定，此頁僅供檢視
        </CardDescription>
      </CardHeader>
      <CardContent>
        <dl className="divide-y divide-border">
          {rows.map((row) => (
            <div
              key={row.label}
              className="flex items-center justify-between py-2.5 text-sm first:pt-0 last:pb-0"
            >
              <dt className="text-muted-foreground">{row.label}</dt>
              <dd className="font-medium tabular-nums">{row.value}</dd>
            </div>
          ))}
        </dl>
        <Badge variant="outline" className="mt-3">
          唯讀 · 需修改設定並重啟服務
        </Badge>
      </CardContent>
    </Card>
  );
}
