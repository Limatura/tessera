import { IconLink, IconUnlink } from "@tabler/icons-react";
import { getRouteApi } from "@tanstack/react-router";
import { Badge } from "@tessera/ui/components/badge";
import { Button } from "@tessera/ui/components/button";
import { Skeleton } from "@tessera/ui/components/skeleton";
import { toast } from "@tessera/ui/components/toast";
import type * as React from "react";

import { ProviderIcon } from "@/components/provider-icon";
import { SignInAgain } from "@/components/sign-in-again";
import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import type { Identity } from "@/hooks/use-identity-providers";
import {
  useIdentities,
  useProviders,
  useUnlinkIdentity,
} from "@/hooks/use-identity-providers";
import { apiErrorMessage } from "@/lib/api-error";
import { oidcErrorMessage, startLink } from "@/lib/api/identity-provider";
import * as m from "@/paraglide/messages";

import { EmptyBox, Section } from "./section";

/** 直接 import Route 會跟 index.tsx 形成循環依賴（同 login-form）。 */
const routeApi = getRouteApi("/_site/user/");

/** 409 是「這是最後一個登入方式」（`last_sign_in_method`）——後端算過，前端不知道有沒有密碼。 */
function errorMessage(error: unknown): string {
  return apiErrorMessage(error, m.common_try_again());
}

/**
 * 一家一列，沒綁的也列出來——這一區要回答「我可以用哪些方式進來」，只畫已綁的
 * 話沒綁過的人會看到空框，不知道還能綁什麼。一個帳號可同時綁多家，所以是清單
 * 而不是單選。
 */
export function IdentitiesSection() {
  const providers = useProviders();
  const identities = useIdentities();
  const unlink = useUnlinkIdentity();
  const formatDate = useDateTimeFormatter();

  const handleUnlink = (provider: string) => {
    unlink.mutate(provider, {
      onError: (error) => {
        toast.add({
          description: errorMessage(error),
          title: m.user_identities_unlink_failed(),
          type: "error",
        });
      },
      onSuccess: () =>
        toast.add({ title: m.user_identities_unlink_done(), type: "success" }),
    });
  };

  if (providers.isPending || identities.isPending) {
    return (
      <IdentitiesShell>
        <div className="space-y-3">
          <Skeleton className="h-16 w-full" />
          <Skeleton className="h-16 w-full" />
        </div>
      </IdentitiesShell>
    );
  }

  if (providers.isError || identities.isError) {
    return (
      <IdentitiesShell>
        <p className="text-destructive text-sm">{m.user_identities_error()}</p>
      </IdentitiesShell>
    );
  }

  const enabled = providers.data?.filter((provider) => provider.enabled) ?? [];
  const linked = identities.data ?? [];

  /*
   * 站台關掉某家後，已綁的身分還在資料庫裡（仍算在 is_last_way_in 裡）。不畫
   * 出來會讓人以為自己沒綁過卻又解不掉，所以仍要列出，只是沒有「綁定」鍵。
   */
  const orphaned = linked.filter(
    (identity) =>
      !enabled.some((provider) => provider.provider === identity.provider)
  );

  if (enabled.length === 0 && orphaned.length === 0) {
    return (
      <IdentitiesShell>
        <EmptyBox icon={<IconLink size={20} />}>
          {m.user_identities_none_configured()}
        </EmptyBox>
      </IdentitiesShell>
    );
  }

  return (
    <IdentitiesShell>
      <ul className="space-y-3">
        {enabled.map((provider) => (
          <IdentityRow
            displayName={provider.display_name}
            formatDate={formatDate}
            identity={linked.find(
              (item) => item.provider === provider.provider
            )}
            isBusy={unlink.isPending && unlink.variables === provider.provider}
            key={provider.provider}
            onUnlink={handleUnlink}
            provider={provider.provider}
          />
        ))}
        {orphaned.map((identity) => (
          <IdentityRow
            // 關掉之後後端不再回 display_name，只剩代碼可用。
            displayName={identity.provider}
            formatDate={formatDate}
            identity={identity}
            isBusy={unlink.isPending && unlink.variables === identity.provider}
            isRetired
            key={identity.provider}
            onUnlink={handleUnlink}
            provider={identity.provider}
          />
        ))}
      </ul>
    </IdentitiesShell>
  );
}

/**
 * 標題、說明、失敗訊息抽出來共用，免得三個 return 分支只有部分能看到訊息。
 *
 * 綁定失敗時後端把人送回這頁並帶上代碼，放在這一區而不是頁首，讓訊息離剛按的
 * 綁定鍵愈近愈好；認不得的代碼退回通用訊息。
 *
 * `sign_in_again` 不算失敗：綁定要求 10 分鐘內登入過（#435），補救是再登入
 * 一次，所以給按鈕而非紅字——回來後再按一次綁定就會成功。
 */
function IdentitiesShell({ children }: { children: React.ReactNode }) {
  const { oidc_error: code } = routeApi.useSearch();
  const error =
    code && code !== "sign_in_again"
      ? (oidcErrorMessage(code) ?? m.oidc_error_generic())
      : null;

  return (
    <Section
      description={m.user_identities_description()}
      title={m.user_identities_title()}
    >
      {code === "sign_in_again" ? (
        <SignInAgain message={m.oidc_error_sign_in_again()} />
      ) : null}
      {error ? (
        <p
          className="rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-destructive text-sm"
          role="alert"
        >
          {error}
        </p>
      ) : null}
      {children}
    </Section>
  );
}

function IdentityRow({
  provider,
  displayName,
  identity,
  formatDate,
  onUnlink,
  isBusy,
  isRetired = false,
}: {
  provider: string;
  displayName: string;
  identity: Identity | undefined;
  formatDate: (value: string | null | undefined) => string;
  onUnlink: (provider: string) => void;
  isBusy: boolean;
  isRetired?: boolean;
}) {
  const isLastWayIn = identity?.is_last_way_in ?? false;

  return (
    /* 跟 session-row 一樣用 flex-wrap：手機上按鈕換行變滿版，桌機上同一列。 */
    <li className="flex flex-wrap items-start gap-x-3 gap-y-3 rounded-lg border p-3">
      <span className="mt-0.5 shrink-0 text-muted-foreground">
        <ProviderIcon provider={provider} size={18} />
      </span>

      <div className="min-w-0 flex-1 basis-48 space-y-1">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <span className="font-medium text-sm">{displayName}</span>
          {isRetired ? (
            <Badge variant="outline">{m.user_identities_retired()}</Badge>
          ) : null}
        </div>
        <p className="text-muted-foreground text-xs">
          {identity
            ? m.user_identities_linked_at({
                date: formatDate(identity.created_at),
              })
            : m.user_identities_not_linked()}
        </p>
        {/* 鎖起來的原因要寫在旁邊，不能只靠 disabled——disabled 按鈕收不到 hover，觸控裝置上開不了 tooltip，等於沒解釋。 */}
        {isLastWayIn ? (
          <p className="text-muted-foreground text-xs">
            {m.user_identities_last_way_in()}
          </p>
        ) : null}
      </div>

      {identity ? (
        <Button
          className="w-full sm:w-auto"
          disabled={isBusy || isLastWayIn}
          onClick={() => onUnlink(provider)}
          size="sm"
          type="button"
          variant="outline"
        >
          <IconUnlink size={16} />
          {m.user_identities_unlink()}
        </Button>
      ) : (
        <Button
          className="w-full sm:w-auto"
          // 導頁而非 fetch：後端回 303，要瀏覽器自己跟到 provider 去。
          onClick={() => startLink(provider)}
          size="sm"
          type="button"
          variant="outline"
        >
          <IconLink size={16} />
          {m.user_identities_link()}
        </Button>
      )}
    </li>
  );
}
