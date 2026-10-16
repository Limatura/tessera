import { Button } from "@tessera/ui/components/button";
import { Skeleton } from "@tessera/ui/components/skeleton";
import { motion } from "motion/react";

import { ProviderIcon } from "@/components/provider-icon";
import { useProviders } from "@/hooks/use-identity-providers";
import { startSignIn } from "@/lib/api/identity-provider";
import { itemVariants } from "@/lib/motion";
import * as m from "@/paraglide/messages";

/**
 * One button per provider; those with `enabled` false are not drawn.
 *
 * A failed query (including the 404 when accounts are off) draws nothing:
 * password sign-in is still there, and an error here would make the whole
 * page look broken.
 */
export function ProviderButtons({
  invite,
  returnTo,
}: {
  invite?: string;
  /**
   * The path on this site to return to after signing in; the same as password sign-in's
   * `?redirect=`.
   */
  returnTo?: string;
}) {
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
              // Navigate instead of submitting a form: the server answers 303 and the
              // browser follows it to the provider.
              onClick={() =>
                startSignIn(provider.provider, { invite, returnTo })
              }
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
