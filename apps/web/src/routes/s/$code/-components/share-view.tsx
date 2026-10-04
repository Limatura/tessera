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
import { Input } from "@tessera/ui/components/input";
import { Skeleton } from "@tessera/ui/components/skeleton";
import {
  Download,
  ExternalLink,
  Eye,
  EyeOff,
  FileIcon,
  FileText,
  Home,
  Link2,
  Lock,
  SearchX,
  TimerOff,
} from "lucide-react";
import { useEffect, useState } from "react";

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

/**
 * What the revealed card should render for this share. Resolved once,
 * synchronously, from metadata already on hand — no network round trip
 * needed to decide whether to auto-fetch a preview.
 */
type PreviewPlan =
  | { type: "text" }
  | { type: "file"; Component: FilePreviewComponent }
  | { type: "none" };

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

  if (data.kind === "link") {
    return <LinkShare code={code} data={data} password={password} />;
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

/**
 * 連結分享只有在有密碼或有次數限制時才會看到這頁,其他情況伺服器直接 302。
 *
 * 不自動拿票券、也不讀 `/raw` 顯示目的地:兩者都會花掉一次提取,Discord 之類的
 * 爬蟲展開網址就會把一次性連結用掉。所以次數限制要等人按「前往」才花。
 * 有密碼的話人已經輸入過密碼,解鎖本身就是意圖,直接導走。
 */
function LinkShare({
  code,
  data,
  password,
}: {
  code: string;
  data: PublicShare;
  password?: string;
}) {
  const [requested, setRequested] = useState(data.has_password);
  const ticket = useShareTicket(code, password, requested);

  useEffect(() => {
    if (ticket.data) {
      window.location.href = buildTransferUrl(
        code,
        "download",
        ticket.data.ticket
      );
    }
  }, [ticket.data, code]);

  const going = ticket.isFetching || ticket.isSuccess;

  // 失敗後 query 仍是 enabled,再 set 一次不會重抓,要明確 refetch
  const handleGo = () => {
    if (requested) {
      ticket.refetch();
      return;
    }
    setRequested(true);
  };

  return (
    <div className="w-full max-w-sm animate-in fade-in duration-200">
      <Card className="w-full border-border/60 bg-card/60 shadow-2xl shadow-black/5 backdrop-blur-2xl">
        <CardHeader className="items-center text-center">
          <div className="mb-2 flex size-10 items-center justify-center rounded-full bg-muted text-muted-foreground">
            <Link2 className="size-5" />
          </div>
          <CardTitle className="break-words font-medium text-xl tracking-tight">
            {data.title || m.share_link_title()}
          </CardTitle>
          <CardDescription>{m.share_link_description()}</CardDescription>
          <ShareMeta
            expiresAt={data.expires_at}
            usesRemaining={data.uses_remaining}
          />
        </CardHeader>
        <CardContent>
          <Button
            className="w-full"
            disabled={going}
            onClick={handleGo}
            type="button"
          >
            <ExternalLink />
            {going ? m.share_link_redirecting() : m.share_link_go()}
          </Button>
        </CardContent>
      </Card>
    </div>
  );
}

function isApiError(error: unknown): error is ApiError {
  return typeof error === "object" && error !== null && "status" in error;
}

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

/**
 * 分享密碼的長度不固定(後端只規定至少 4 碼),所以用一般的密碼輸入框,
 * 不用固定格數的 OTP 輸入(#357)。
 */
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
  const [showPassword, setShowPassword] = useState(false);

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    onSubmit(password);
  };

  return (
    <form onSubmit={handleSubmit}>
      <FieldGroup>
        <Field data-invalid={error}>
          <div className="relative">
            <Input
              aria-invalid={error}
              aria-label={m.share_password_placeholder()}
              autoComplete="off"
              autoFocus
              className="pr-8"
              onChange={(event) => setPassword(event.target.value)}
              placeholder={m.share_password_placeholder()}
              type={showPassword ? "text" : "password"}
              value={password}
            />
            <button
              aria-label={
                showPassword
                  ? m.upload_result_password_hide()
                  : m.upload_result_password_show()
              }
              className="-translate-y-1/2 absolute top-1/2 right-2 text-current/60 hover:text-current"
              onClick={() => setShowPassword((value) => !value)}
              type="button"
            >
              {showPassword ? (
                <EyeOff className="size-3.5" />
              ) : (
                <Eye className="size-3.5" />
              )}
            </button>
          </div>
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
