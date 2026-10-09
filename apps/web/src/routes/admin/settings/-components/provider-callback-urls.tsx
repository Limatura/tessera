import { IconAlertTriangle, IconCheck, IconCopy } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";
import { Button } from "@tessera/ui/components/button";
import { useState } from "react";

import * as m from "@/paraglide/messages";

import { SettingsNotice } from "./form-fields";

/**
 * 要貼進 provider 後台的那幾個位址。
 *
 * provider 端一定要先登記 redirect URI 才肯把瀏覽器送回來，登記錯一個字元就
 * 是 `refused`，且錯誤出現在對方頁面上不在這裡。以前這串要 operator 自己照
 * `auth/oidc/flow.rs` 的規則手動拼。
 *
 * 位址由 `server.public_url` 決定，不是瀏覽器目前的網址：真正送給 provider
 * 的是前者（見 `flow.rs` 的 `redirect_uri`）。後台常掛在另一個網域或連接埠
 * 下，拿 `window.location` 拼會給出看起來對、但登入時對不上的值。
 */
export function ProviderCallbackUrls({
  publicUrl,
  provider,
  showBackchannel = false,
}: {
  /** `server.public_url` 的現值。空的話什麼都拼不出來。 */
  publicUrl: string;
  /** 路徑裡的那一段：`oidc`、`github`、`discord`。 */
  provider: string;
  /** 只在讀過 discovery、對方也宣告支援時才給——支援的是少數，無條件顯示只是雜訊。 */
  showBackchannel?: boolean;
}) {
  const base = publicUrl.trim().replace(/\/+$/u, "");

  if (base === "") {
    return <NoPublicUrl />;
  }

  return (
    <div className="space-y-3 rounded-md border border-dashed p-3">
      <p className="text-muted-foreground text-sm">
        {m.settings_provider_callback_intro()}
      </p>
      <CopyableUrl
        hint={m.settings_provider_callback_redirect_hint()}
        label={m.settings_provider_callback_redirect_label()}
        url={`${base}/api/v1/auth/${provider}/callback`}
      />
      {showBackchannel ? (
        <CopyableUrl
          hint={m.settings_provider_callback_backchannel_hint()}
          label="Back-channel logout URL"
          url={`${base}/api/v1/auth/oidc/backchannel-logout`}
        />
      ) : null}
    </div>
  );
}

/**
 * `server.public_url` 沒設就什麼都拼不出來，而且那時連登入本身都會失敗
 * （`flow.rs` 回 `NoPublicUrl`），所以這裡講「先去設它」，不顯示半個位址。
 */
function NoPublicUrl() {
  return (
    <SettingsNotice
      icon={<IconAlertTriangle />}
      title={m.settings_provider_callback_no_public_url_title()}
      tone="warning"
    >
      {m.settings_provider_callback_no_public_url_description()}{" "}
      <Link
        className="underline underline-offset-4 hover:text-foreground"
        to="/admin/settings/network"
      >
        {m.nav_settings_network()}
      </Link>
    </SettingsNotice>
  );
}

function CopyableUrl({
  label,
  url,
  hint,
}: {
  label: string;
  url: string;
  hint: string;
}) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // 不是 https，或拒絕了剪貼簿權限。位址還在畫面上，選取複製就行，不用跳錯誤打斷人。
    }
  };

  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between gap-2">
        <span className="font-medium text-sm">{label}</span>
        <Button onClick={handleCopy} size="sm" type="button" variant="ghost">
          {copied ? <IconCheck size={16} /> : <IconCopy size={16} />}
          {copied
            ? m.settings_provider_callback_copied()
            : m.settings_provider_callback_copy()}
        </Button>
      </div>
      {/* break-all：窄視窗下不斷行會撐出橫向捲軸。用 <code> 不用唯讀 input——要的是看跟複製，不是編輯。 */}
      <code className="block break-all rounded bg-muted px-2 py-1.5 font-mono text-xs">
        {url}
      </code>
      <p className="text-muted-foreground text-xs">{hint}</p>
    </div>
  );
}
