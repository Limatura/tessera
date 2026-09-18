import { Badge } from "@tessera/ui/components/badge";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
} from "@tessera/ui/components/field";
import type * as React from "react";

/**
 * 一個面板改不動的設定。
 *
 * 讀得到卻送不出去的 key 不能留在表單裡：`PUT /admin/settings` 把 changes 當一組
 * 驗證，混進一個被拒絕的 key 會讓同一批的合法欄位也一起退回。但整格拿掉又會讓人
 * 以為設定不存在，所以改成顯示現值加上為什麼改不了。
 */
export function ReadOnlySetting({
  label,
  value,
  reason,
  description,
}: {
  label: string;
  /** server 上實際生效的值。SECRET 的 key 讀不到，傳 `null` 就不畫這一行。 */
  value: string | null;
  /** 標在標題旁邊的短原因，例如「需重啟」。 */
  reason: string;
  description: React.ReactNode;
}) {
  return (
    <Field orientation="horizontal">
      <FieldContent>
        <FieldLabel>
          {label}
          <Badge variant="outline">{reason}</Badge>
        </FieldLabel>
        <FieldDescription>{description}</FieldDescription>
      </FieldContent>
      {value !== null && (
        <output className="font-mono text-muted-foreground text-sm tabular-nums">
          {value}
        </output>
      )}
    </Field>
  );
}
