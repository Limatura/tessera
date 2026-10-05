import { Button } from "@tessera/ui/components/button";
import { Skeleton } from "@tessera/ui/components/skeleton";
import { motion } from "motion/react";

import { ProviderIcon } from "@/components/provider-icon";
import { useProviders } from "@/hooks/use-identity-providers";
import { startSignIn } from "@/lib/api/identity-provider";
import { itemVariants } from "@/lib/motion";
import * as m from "@/paraglide/messages";

/**
 * 一家一顆按鈕，`enabled` 為 false 的不畫。
 *
 * 查詢失敗（含帳號功能關掉時的 404）就什麼都不畫 —— 密碼登入本來就還在，
 * 在這裡擺一句錯誤只會讓人以為整頁壞了。
 */
export function ProviderButtons({ invite }: { invite?: string }) {
  const { data, isPending } = useProviders();

  if (isPending) {
    return (
      <motion.div variants={itemVariants}>
        <Skeleton className="h-9 w-full" />
      </motion.div>
    );
  }

  const enabled = data?.filter((provider) => provider.enabled) ?? [];
  if (enabled.length === 0) {
    return null;
  }

  return (
    <>
      <motion.div
        className="flex items-center gap-3 text-muted-foreground text-xs"
        variants={itemVariants}
      >
        <span className="h-px flex-1 bg-border" />
        {m.auth_or_continue_with()}
        <span className="h-px flex-1 bg-border" />
      </motion.div>
      {enabled.map((provider) => (
        <motion.div key={provider.provider} variants={itemVariants}>
          <motion.div whileTap={{ scale: 0.98 }}>
            <Button
              className="w-full"
              // 導頁而不是送出表單：後端回 303 要瀏覽器自己跟到 provider 去。
              onClick={() => startSignIn(provider.provider, invite)}
              type="button"
              variant="outline"
            >
              <ProviderIcon provider={provider.provider} />
              {m.auth_continue_with({ provider: provider.display_name })}
            </Button>
          </motion.div>
        </motion.div>
      ))}
    </>
  );
}
