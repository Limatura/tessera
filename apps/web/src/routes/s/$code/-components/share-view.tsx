import { useNavigate } from "@tanstack/react-router";
import type { components } from "@tessera/api/schema";
import { Button } from "@tessera/ui/components/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import { Field, FieldError, FieldGroup } from "@tessera/ui/components/field";
import {
  InputOTP,
  InputOTPGroup,
  InputOTPSeparator,
} from "@tessera/ui/components/input-otp";
import { Skeleton } from "@tessera/ui/components/skeleton";
import { cn } from "@tessera/ui/lib/utils";
import { OTPInputContext } from "input-otp";
import {
  Download,
  FileIcon,
  FileText,
  Home,
  Lock,
  SearchX,
  TimerOff,
} from "lucide-react";
import { useContext, useEffect, useState } from "react";

import { buildTransferUrl, useShare, useShareTicket } from "@/lib/api/share";
import { formatBytes } from "@/lib/format";
import * as m from "@/paraglide/messages";
import { resolveFilePreview } from "@/routes/s/$code/-components/file-previews/registry";
import type { FilePreviewComponent } from "@/routes/s/$code/-components/file-previews/types";
import { TextPreview } from "@/routes/s/$code/-components/text-preview";

type PublicShare = components["schemas"]["PublicShareResponse"];

interface ApiError {
  status?: number;
}

function isApiError(error: unknown): error is ApiError {
  return typeof error === "object" && error !== null && "status" in error;
}

/**
 * What the revealed card should render for this share. Resolved once,
 * synchronously, from metadata already on hand — no network round trip
 * needed to decide whether to auto-fetch a preview.
 */
type PreviewPlan =
  | { type: "text" }
  | { type: "file"; Component: FilePreviewComponent }
  | { type: "none" };

function resolvePreviewPlan(data: PublicShare): PreviewPlan {
  if (data.kind === "text") {
    return { type: "text" };
  }
  const filename = data.files[0]?.filename;
  const Component = filename ? resolveFilePreview(filename) : null;
  return Component ? { Component, type: "file" } : { type: "none" };
}

function StatusCard({
  description,
  icon,
  title,
  action,
}: {
  description: string;
  icon: React.ReactNode;
  title: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="w-full max-w-sm animate-in fade-in duration-300">
      <Card className="w-full border-border/60 bg-card/60 shadow-2xl shadow-black/5 backdrop-blur-2xl">
        <CardHeader className="items-center text-center">
          <div className="mb-2 flex size-10 items-center justify-center rounded-full bg-muted text-muted-foreground">
            {icon}
          </div>
          <CardTitle className="font-medium text-xl tracking-tight">
            {title}
          </CardTitle>
          <CardDescription>{description}</CardDescription>
        </CardHeader>
        {action && <CardContent>{action}</CardContent>}
      </Card>
    </div>
  );
}

function ShareSkeleton() {
  return (
    <Card className="w-full max-w-sm border-border/60 bg-card/60 shadow-2xl shadow-black/5 backdrop-blur-2xl">
      <CardHeader>
        <Skeleton className="h-4 w-24" />
        <Skeleton className="mt-3 h-6 w-40" />
        <Skeleton className="mt-2 h-4 w-32" />
      </CardHeader>
      <CardContent>
        <Skeleton className="h-7 w-full" />
      </CardContent>
    </Card>
  );
}

const PASSWORD_OTP_LENGTH = 8;

/** Same slot as the shared component, but renders a dot instead of the
 * character — this field holds a password, not a code meant to be read. */
function MaskedOTPSlot({ index }: { index: number }) {
  const context = useContext(OTPInputContext);
  const { char, hasFakeCaret, isActive } = context?.slots[index] ?? {};

  return (
    <div
      className={cn(
        "relative flex size-9 items-center justify-center border-y border-r border-input bg-input/20 text-sm outline-none transition-all first:rounded-l-md first:border-l last:rounded-r-md dark:bg-input/30",
        isActive && "z-10 border-ring ring-2 ring-ring/30"
      )}
      data-active={isActive}
    >
      {char && <span className="size-1.5 rounded-full bg-foreground" />}
      {hasFakeCaret && (
        <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
          <div className="h-4 w-px animate-caret-blink bg-foreground duration-1000" />
        </div>
      )}
    </div>
  );
}

function PasswordForm({
  error,
  onSubmit,
  pending,
}: {
  error: boolean;
  onSubmit: (password: string) => void;
  pending: boolean;
}) {
  const [password, setPassword] = useState("");

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    onSubmit(password);
  };

  return (
    <form onSubmit={handleSubmit}>
      <FieldGroup>
        <Field data-invalid={error}>
          <InputOTP
            aria-invalid={error}
            aria-label={m.share_password_placeholder()}
            autoFocus
            maxLength={PASSWORD_OTP_LENGTH}
            onChange={setPassword}
            value={password}
          >
            <InputOTPGroup className="mx-auto">
              {Array.from({ length: 4 }, (_, index) => (
                <MaskedOTPSlot index={index} key={index} />
              ))}
            </InputOTPGroup>
            <InputOTPSeparator />
            <InputOTPGroup className="mx-auto">
              {Array.from({ length: 4 }, (_, index) => (
                <MaskedOTPSlot index={index + 4} key={index + 4} />
              ))}
            </InputOTPGroup>
          </InputOTP>
          {error && (
            <FieldError className="text-center">
              {m.share_password_incorrect()}
            </FieldError>
          )}
        </Field>
        <Button
          className="w-full"
          disabled={pending || password.length === 0}
          type="submit"
        >
          {pending ? m.share_unlocking() : m.share_unlock()}
        </Button>
      </FieldGroup>
    </form>
  );
}

function ShareMeta({
  expiresAt,
  sizeBytes,
  usesRemaining,
}: {
  expiresAt?: string | null;
  sizeBytes?: number | null;
  usesRemaining?: number | null;
}) {
  const parts = [
    sizeBytes === null || sizeBytes === undefined
      ? null
      : formatBytes(sizeBytes),
    usesRemaining === null || usesRemaining === undefined
      ? m.share_uses_unlimited()
      : m.share_uses_remaining({ count: usesRemaining }),
    expiresAt
      ? m.share_expires_at({ date: new Date(expiresAt).toLocaleString() })
      : m.share_never_expires(),
  ].filter(Boolean);

  return <p className="text-muted-foreground text-xs">{parts.join(" · ")}</p>;
}

function RevealedShare({
  code,
  data,
  password,
}: {
  code: string;
  data: PublicShare;
  password?: string;
}) {
  const plan = resolvePreviewPlan(data);
  const [manualRequested, setManualRequested] = useState(false);
  const ticket = useShareTicket(
    code,
    password,
    plan.type !== "none" || manualRequested
  );

  useEffect(() => {
    if (plan.type === "none" && manualRequested && ticket.data) {
      window.location.href = buildTransferUrl(
        code,
        "download",
        ticket.data.ticket
      );
    }
  }, [plan.type, manualRequested, ticket.data, code]);

  const handleDownloadClick = () => {
    if (ticket.data) {
      window.location.href = buildTransferUrl(
        code,
        "download",
        ticket.data.ticket
      );
      return;
    }
    setManualRequested(true);
  };

  return (
    <div className="flex flex-col gap-4">
      {plan.type === "text" &&
        (ticket.data ? (
          <TextPreview
            language={data.language}
            sizeBytes={data.size_bytes}
            url={buildTransferUrl(code, "raw", ticket.data.ticket)}
          />
        ) : (
          <Skeleton className="h-48 w-full rounded-md" />
        ))}

      {plan.type === "file" &&
        (ticket.data ? (
          <plan.Component
            filename={data.files[0]?.filename ?? ""}
            url={buildTransferUrl(code, "download", ticket.data.ticket)}
          />
        ) : (
          <Skeleton className="h-48 w-full rounded-md" />
        ))}

      {plan.type === "none" && (
        <div className="flex flex-col items-center gap-2 rounded-md border border-border/60 bg-muted/20 p-8">
          <FileIcon className="size-8 text-muted-foreground" />
          <p className="text-muted-foreground text-sm">
            {m.share_no_preview()}
          </p>
        </div>
      )}

      <Button
        className="w-full"
        disabled={ticket.isFetching}
        onClick={handleDownloadClick}
        type="button"
      >
        <Download />
        {ticket.isFetching ? m.share_preparing() : m.share_download()}
      </Button>
    </div>
  );
}

export function ShareView({ code }: { code: string }) {
  const navigate = useNavigate();
  const [password, setPassword] = useState<string | undefined>();
  const share = useShare(code, password);

  if (share.isPending) {
    return <ShareSkeleton />;
  }

  const status =
    share.isError && isApiError(share.error) ? share.error.status : undefined;

  // A wrong password re-triggers the same 401 the initial locked lookup
  // gets, so the only way to tell them apart is whether a password was
  // already tried.
  const wrongPassword = status === 401 && password !== undefined;

  if (share.isError && !wrongPassword) {
    if (status === 429) {
      return (
        <StatusCard
          action={
            <Button
              className="w-full"
              onClick={() => navigate({ to: "/s" })}
              type="button"
              variant="ghost"
            >
              {m.share_back_home()}
            </Button>
          }
          description={m.share_rate_limited_description()}
          icon={<TimerOff className="size-5" />}
          title={m.share_rate_limited_title()}
        />
      );
    }
    if (status === 410) {
      return (
        <StatusCard
          action={
            <Button
              className="w-full"
              onClick={() => navigate({ to: "/s" })}
              type="button"
              variant="ghost"
            >
              {m.share_back_home()}
            </Button>
          }
          description={m.share_gone_description()}
          icon={<TimerOff className="size-5" />}
          title={m.share_gone_title()}
        />
      );
    }
    return (
      <StatusCard
        action={
          <Button
            className="w-full"
            onClick={() => navigate({ to: "/" })}
            type="button"
            variant="default"
          >
            <Home />
            {m.share_back_home()}
          </Button>
        }
        description={m.share_not_found_description()}
        icon={<SearchX className="size-5" />}
        title={m.share_not_found_title()}
      />
    );
  }

  const { data } = share;
  const locked =
    wrongPassword || (data ? data.has_password && !data.unlocked : true);

  if (locked) {
    return (
      <div className="w-full max-w-sm animate-in fade-in duration-200">
        <Card className="w-full border-border/60 bg-card/60 shadow-2xl shadow-black/5 backdrop-blur-2xl">
          <CardHeader className="items-center text-center">
            <div className="mb-2 flex size-10 items-center justify-center rounded-full bg-muted text-muted-foreground">
              <Lock className="size-5" />
            </div>
            <CardTitle className="font-medium text-xl tracking-tight">
              {m.share_locked_title()}
            </CardTitle>
            <CardDescription>{m.share_locked_description()}</CardDescription>
          </CardHeader>
          <CardContent>
            <PasswordForm
              error={wrongPassword}
              onSubmit={setPassword}
              pending={share.isFetching}
            />
          </CardContent>
        </Card>
      </div>
    );
  }

  if (!data) {
    return null;
  }

  return (
    <div className="w-full max-w-xl animate-in fade-in duration-200">
      <Card className="w-full border-border/60 bg-card/60 shadow-2xl shadow-black/5 backdrop-blur-2xl">
        <CardHeader className="items-center text-center">
          <div className="mb-2 flex size-10 items-center justify-center rounded-full bg-muted text-muted-foreground">
            {data.kind === "text" ? (
              <FileText className="size-5" />
            ) : (
              <FileIcon className="size-5" />
            )}
          </div>
          <CardTitle className="font-medium text-xl tracking-tight">
            {data.title || data.files[0]?.filename || m.share_untitled()}
          </CardTitle>
          <ShareMeta
            expiresAt={data.expires_at}
            sizeBytes={data.size_bytes}
            usesRemaining={data.uses_remaining}
          />
        </CardHeader>
        <CardContent>
          <RevealedShare code={code} data={data} password={password} />
        </CardContent>
      </Card>
    </div>
  );
}
