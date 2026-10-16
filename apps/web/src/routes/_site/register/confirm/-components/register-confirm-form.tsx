import { Link, useNavigate } from "@tanstack/react-router";
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
    // Missing, used or expired: one message for all three, as on the backend.
    // Telling them apart would tell whoever holds a used link something more.
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
    // Taken while the link waited: the username clashed, or the email already
    // has an account (the backend folds the latter into
    // registration_link_unusable, but username_taken is a real 409).
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
  // StrictMode runs effects twice; once a token is spent, never send it again.
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
    // confirmRegistration is a stable mutate object; rerun only when the token changes.
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
                <Link
                  className={cn(
                    buttonVariants({ variant: "outline" }),
                    "w-full"
                  )}
                  to="/register"
                >
                  {m.auth_register_confirm_retry()}
                </Link>
              </CardContent>
            ) : null}
          </Card>
        </motion.div>
      </motion.div>
    </div>
  );
}
