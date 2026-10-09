import {
  IconAlertCircle,
  IconCircleCheck,
  IconLoader2,
} from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";
import {
  Field,
  FieldDescription,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import { useState } from "react";

import type { ReadResult } from "@/lib/admin/oidc-discovery";
import { readDiscovery } from "@/lib/admin/oidc-discovery";
import * as m from "@/paraglide/messages";

/**
 * 貼上 discovery URL，請伺服器讀一次文件，把裡面的值填進下面的欄位。
 *
 * 值是讀出來的，不是從網址推的：文件裡的 `issuer` 才是權威來源，規範要求它與
 * `iss` claim 一字不差。剪網址字尾多數時候結果一樣，但那是猜的。
 *
 * 讀取走後端（見 `lib/oidc-discovery.ts`）：不少 provider 不開放瀏覽器跨來源
 * 讀取，而伺服器抓得到就代表登入路徑也抓得到。
 *
 * 能帶入的只有 issuer 跟 scopes —— 端點後端自己會從同一份文件讀（見
 * `discovery.rs`），根本沒有欄位存它們。Client ID／Secret 是 provider 後台另外
 * 發的，不在這份文件裡，要自己填。
 */
export function OidcDiscoveryField({
  issuer,
  scopes,
  onApply,
}: {
  issuer: string;
  scopes: string;
  onApply: (values: {
    issuer: string;
    scopes?: string;
    backchannelLogout: boolean;
    claimsSupported: string[] | null;
  }) => void;
}) {
  const [value, setValue] = useState("");
  const [result, setResult] = useState<ReadResult | null>(null);
  const [isReading, setIsReading] = useState(false);

  // 這格空著就讀下面 Issuer 的現值 —— 已經設定好的站台是來重新確認的，
  // 不該逼人把網址再貼一次。
  const target = value.trim() === "" ? issuer.trim() : value.trim();

  const handleRead = async () => {
    if (target === "" || isReading) {
      return;
    }
    setIsReading(true);
    const read = await readDiscovery(
      target,
      scopes.split(/\s+/u).filter(Boolean)
    );
    setIsReading(false);
    setResult(read);

    if (read.ok) {
      onApply({
        backchannelLogout: read.backchannelLogout,
        claimsSupported: read.claimsSupported,
        issuer: read.issuer,
        // 沒宣告 scopes_supported 就不動使用者填的 —— 沒宣告不代表不支援。
        scopes: read.scopes ? read.scopes.join(" ") : undefined,
      });
    }
  };

  return (
    <div className="space-y-3 rounded-md border border-dashed p-3">
      <Field>
        <FieldLabel htmlFor="oidc-discovery-url">
          {m.settings_oidc_discovery_field_url_label()}
        </FieldLabel>
        <div className="flex flex-col gap-2 sm:flex-row">
          <Input
            autoComplete="off"
            className="sm:flex-1"
            id="oidc-discovery-url"
            onChange={(event) => setValue(event.target.value)}
            placeholder="https://auth.example.com/.well-known/openid-configuration"
            value={value}
          />
          <Button
            className="sm:w-auto"
            disabled={target === "" || isReading}
            onClick={handleRead}
            type="button"
            variant="outline"
          >
            {isReading ? (
              <IconLoader2 className="animate-spin" size={16} />
            ) : null}
            {m.settings_oidc_discovery_field_read_button()}
          </Button>
        </div>
        <FieldDescription>
          {m.settings_oidc_discovery_field_description()}
        </FieldDescription>
      </Field>

      {result ? <ReadMessage result={result} /> : null}
    </div>
  );
}

function ReadMessage({ result }: { result: ReadResult }) {
  if (!result.ok) {
    return (
      <div
        className="flex gap-2 rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm"
        role="alert"
      >
        <IconAlertCircle
          className="mt-0.5 size-4 shrink-0 text-destructive"
          size={16}
        />
        <p>{result.reason}</p>
      </div>
    );
  }

  const missingEmail =
    result.supportedScopes !== null &&
    !result.supportedScopes.includes("email");

  return (
    <output className="flex gap-2 rounded-md border border-emerald-500/40 bg-emerald-500/10 p-3 text-sm">
      <IconCircleCheck
        className="mt-0.5 size-4 shrink-0 text-emerald-600 dark:text-emerald-500"
        size={16}
      />
      <div className="space-y-1">
        <p className="font-medium">
          {m.settings_oidc_discovery_field_read_success()}
        </p>
        {result.supportedScopes ? (
          <p className="text-muted-foreground">
            {m.settings_oidc_discovery_field_supported_scopes({
              scopes: result.supportedScopes.join("、"),
            })}
          </p>
        ) : (
          <p className="text-muted-foreground">
            {m.settings_oidc_discovery_field_no_scopes_declared()}
          </p>
        )}
        {/*
          email 拿不到就沒有帳號可建，後端會以 no_address 拒絕每一次登入。
          這件事值得當場講，不然要到第一個人登入失敗才會發現。
        */}
        {missingEmail ? (
          <p className="font-medium text-amber-600 dark:text-amber-500">
            {m.settings_oidc_discovery_field_missing_email()}
          </p>
        ) : null}
      </div>
    </output>
  );
}
