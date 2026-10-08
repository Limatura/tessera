import { useNavigate } from "@tanstack/react-router";
import type { components } from "@tessera/api/schema";
import { useConfirmRegistration } from "@tessera/auth/hooks";
import { buttonVariants } from "@tessera/ui/components/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import { cn } from "@tessera/ui/lib/utils";
import { motion } from "motion/react";
import { useEffect, useMemo, useRef } from "react";

import { containerVariants, itemVariants } from "@/lib/motion";
import * as m from "@/paraglide/messages";

type ErrorCode = components["schemas"]["ErrorBody"]["code"];

function errorContent(code: ErrorCode | undefined): {
  description: string;
  title: string;
} {
  switch (code) {
    // 連結不存在、已用過或過期 —— 都是同一句,見後端註解:三者分開說會讓
    // 用過的連結的人多學到一件事。
    case "registration_link_unusable": {
      return {
        description: m.auth_register_confirm_link_unusable_description(),
        title: m.auth_register_confirm_link_unusable_title(),
      };
    }
    case "registration_closed": {
      return {
        description: m.auth_register_confirm_closed_description(),
        title: m.auth_register_confirm_closed_title(),
      };
    }
    // 等待期間被別人用掉:username 撞名,或 email 已經有帳號(後端把後者併進
    // registration_link_unusable,但 username_taken 是真的 409)。
    case "username_taken": {
      return {
        description: m.auth_register_confirm_taken_description(),
        title: m.auth_register_confirm_taken_title(),
      };
    }
    default: {
      return {
        description: m.auth_register_confirm_generic_description(),
        title: m.auth_register_confirm_generic_title(),
      };
    }
  }
}

export function RegisterConfirmForm({
  className,
  token,
  ...props
}: React.ComponentProps<"div"> & { token: string | null }) {
  const navigate = useNavigate();
  const confirmRegistration = useConfirmRegistration();
  // StrictMode 下 effect 會跑兩次；token 一旦被消費過就不要再送第二次請求。
  const requested = useRef(false);

  useEffect(() => {
    if (!token || requested.current) {
      return;
    }
    requested.current = true;
    confirmRegistration.mutate(
      { token },
      {
        onSuccess: () => {
          navigate({ to: "/" });
        },
      }
    );
    // confirmRegistration 是穩定的 mutate 物件；只在 token 變化時重跑。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [token]);

  const content = useMemo(() => {
    if (!token) {
      return {
        description: m.auth_register_confirm_missing_token_description(),
        title: m.auth_register_confirm_missing_token_title(),
      };
    }
    if (confirmRegistration.isError) {
      const error = confirmRegistration.error as unknown as
        | components["schemas"]["ErrorBody"]
        | null;
      return errorContent(error?.code);
    }
    return null;
  }, [token, confirmRegistration.isError, confirmRegistration.error]);

  return (
    <div className={cn("flex flex-col gap-6", className)} {...props}>
      <motion.div
        animate="visible"
        initial="hidden"
        variants={containerVariants}
      >
        <motion.div variants={itemVariants}>
          <Card className="border-border/60 bg-card/60 shadow-2xl shadow-black/5 backdrop-blur-2xl">
            <CardHeader>
              <motion.div variants={itemVariants}>
                <CardTitle className="font-medium text-xl tracking-tight">
                  {content
                    ? content.title
                    : m.auth_register_confirm_pending_title()}
                </CardTitle>
              </motion.div>
              <motion.div variants={itemVariants}>
                <CardDescription>
                  {content
                    ? content.description
                    : m.auth_register_confirm_pending_description()}
                </CardDescription>
              </motion.div>
            </CardHeader>
            {content ? (
              <CardContent>
                <a
                  className={cn(
                    buttonVariants({ variant: "outline" }),
                    "w-full"
                  )}
                  href="/register"
                >
                  {m.auth_register_confirm_retry()}
                </a>
              </CardContent>
            ) : null}
          </Card>
        </motion.div>
      </motion.div>
    </div>
  );
}
