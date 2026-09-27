import { Link } from "@tanstack/react-router";
import type { components } from "@tessera/api/schema";
import { buttonVariants } from "@tessera/ui/components/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import { motion } from "motion/react";
import { useEffect, useRef } from "react";

import { useConfirmEmailVerification } from "@/hooks/use-account";
import { useSession } from "@/hooks/use-auth";
import { containerVariants, itemVariants } from "@/lib/motion";
import * as m from "@/paraglide/messages";

type ErrorBody = components["schemas"]["ErrorBody"];

/**
 * 一打開就拿 token 去確認,不用再按一次:點信裡的連結本身就是那個動作。
 * 信件掃描器預抓連結只會 GET 這一頁,不會執行 JS,所以不會替人把 token 用掉。
 */
export function VerifyEmailCard({ token }: { token: string | undefined }) {
  const confirm = useConfirmEmailVerification();
  const { mutate } = confirm;
  const { data: session } = useSession();

  // StrictMode 會把 effect 跑兩次,token 只能用一次 —— 第二次一定是 400。
  const started = useRef(false);

  useEffect(() => {
    if (!token || started.current) {
      return;
    }
    started.current = true;
    mutate(token);
    // token 是一次性的憑證,不留在網址上:會進瀏覽紀錄、被截圖、被 Referer 帶走。
    // 直接改 history 而不是 navigate,router 的 search 才不會跟著變成沒有 token
    // (同 reset-password 的做法)。
    window.history.replaceState(null, "", "/verify-email");
  }, [token, mutate]);

  const { title, description } = describe(
    token,
    confirm.status,
    confirm.error as Partial<ErrorBody> | null
  );

  return (
    <motion.div animate="visible" initial="hidden" variants={containerVariants}>
      <motion.div variants={itemVariants}>
        <Card className="border-border/60 bg-card/60 shadow-2xl shadow-black/5 backdrop-blur-2xl">
          <CardHeader>
            <CardTitle className="font-medium text-xl tracking-tight">
              {title}
            </CardTitle>
            <CardDescription aria-live="polite">{description}</CardDescription>
          </CardHeader>
          {confirm.isPending ? null : (
            <CardContent>
              {/* 信常在另一台裝置打開,那邊沒登入就送去登入,不是帳號設定。 */}
              {session ? (
                <Link
                  className={buttonVariants({ className: "w-full" })}
                  search={{ tab: "account" }}
                  to="/user"
                >
                  {m.user_verify_email_to_account()}
                </Link>
              ) : (
                <Link
                  className={buttonVariants({ className: "w-full" })}
                  to="/login"
                >
                  {m.auth_sign_in()}
                </Link>
              )}
            </CardContent>
          )}
        </Card>
      </motion.div>
    </motion.div>
  );
}

function describe(
  token: string | undefined,
  status: "idle" | "pending" | "success" | "error",
  error: Partial<ErrorBody> | null
): { title: string; description: string } {
  if (!token) {
    return {
      description: m.user_verify_email_missing(),
      title: m.auth_error(),
    };
  }
  if (status === "success") {
    return {
      description: m.user_verify_email_done_description(),
      title: m.user_verify_email_done(),
    };
  }
  if (status === "error") {
    // 400 把「不存在、用過、過期、寄給舊地址」併成一句,server 刻意不分,這裡也不分。
    return {
      description:
        error?.code === "verification_link_unusable"
          ? m.user_verify_email_unusable()
          : (error?.message ?? m.common_try_again()),
      title: m.auth_error(),
    };
  }
  return {
    description: m.user_verify_email_pending(),
    title: m.user_verify_email_title(),
  };
}
