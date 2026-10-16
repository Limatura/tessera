import { Link } from "@tanstack/react-router";
import type { components } from "@tessera/api/schema";
import { useSession } from "@tessera/auth/hooks";
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
import { apiErrorMessage } from "@/lib/api-error";
import { containerVariants, itemVariants } from "@/lib/motion";
import * as m from "@/paraglide/messages";

type ErrorBody = components["schemas"]["ErrorBody"];

/**
 * Confirms the token on load, without another click: following the link in the mail is
 * the action. Mail scanners that prefetch links only GET this page and run no JS, so
 * they cannot spend the token.
 */
export function VerifyEmailCard({ token }: { token: string | undefined }) {
  const confirm = useConfirmEmailVerification();
  const { mutate } = confirm;
  const { data: session } = useSession();

  // StrictMode runs effects twice and the token is single-use: a second call is always a 400.
  const started = useRef(false);

  useEffect(() => {
    if (!token || started.current) {
      return;
    }
    started.current = true;
    mutate(token);
    // The token is a one-time credential and must not stay in the URL, where it ends up
    // in history, screenshots and Referer headers. Rewrite history directly instead of
    // calling navigate, so the router's search keeps the token (as in reset-password).
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
              {/* The mail is often opened on another device; signed out there,
                 go to sign-in, not account settings. */}
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
  // The token leaves the URL once the request starts, so "missing" only means
  // something before any request was made. After that, the status is the answer.
  if (!token && status === "idle") {
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
    // A 400 covers missing, used, expired and sent-to-an-old-address alike. The
    // server keeps them together, and so does this.
    return {
      description:
        error?.code === "verification_link_unusable"
          ? m.user_verify_email_unusable()
          : apiErrorMessage(error, m.common_try_again()),
      title: m.auth_error(),
    };
  }
  return {
    description: m.user_verify_email_pending(),
    title: m.user_verify_email_title(),
  };
}
