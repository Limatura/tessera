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
import { Skeleton } from "@tessera/ui/components/skeleton";
import {
  FileIcon,
  FileText,
  Home,
  Lock,
  SearchX,
  ShieldEllipsis,
  TimerOff,
} from "lucide-react";
import { useState } from "react";

import { usePasswordKey } from "@/hooks/use-password-key";
import { useShare } from "@/hooks/use-share";
import { languageLabel } from "@/lib/language-labels";
import * as m from "@/paraglide/messages";

import { EncryptedShare } from "./encrypted-share";
import { LinkShare } from "./link-share";
import { PasswordForm } from "./password-form";
import { RevealedShare } from "./revealed-share";
import { ShareMeta } from "./share-meta";
import { StatusCard } from "./status-card";

type PublicShare = components["schemas"]["PublicShareResponse"];

interface ApiError {
  status?: number;
}

export function ShareView({ code }: { code: string }) {
  const navigate = useNavigate();
  const [password, setPassword] = useState<string | undefined>();
  const unlock = usePasswordKey(code, setPassword);
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
      <LockedCard
        deriving={unlock.deriving}
        error={wrongPassword}
        onSubmit={unlock.submit}
        pending={share.isFetching}
      />
    );
  }

  if (!data) {
    return null;
  }

  // Until a verdict, download and unlock answer 423. `useShare` polls and
  // swaps in the normal view once the scan is done.
  if (data.scanning) {
    return (
      <StatusCard
        description={m.share_scanning_description()}
        icon={<ShieldEllipsis className="size-5 animate-pulse" />}
        title={m.share_scanning_title()}
      />
    );
  }

  if (data.kind === "link") {
    return <LinkShare code={code} data={data} password={password} />;
  }

  if (data.encryption) {
    return (
      <EncryptedShare
        code={code}
        data={data}
        password={password}
        passwordKey={unlock.key}
      />
    );
  }

  return <PlainShareCard code={code} data={data} password={password} />;
}

function isApiError(error: unknown): error is ApiError {
  return typeof error === "object" && error !== null && "status" in error;
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
 * With no title or file name (most pasted code), build a name from the language,
 * which says more than "Untitled share". An encrypted share seals its language, so
 * it falls back to "Untitled share".
 */
function fallbackTitle(data: { kind: string; language?: string | null }) {
  if (data.kind !== "text") {
    return m.share_untitled();
  }
  const label =
    data.language && data.language !== "plaintext"
      ? languageLabel(data.language)
      : null;
  return label
    ? m.share_untitled_code({ language: label })
    : m.share_untitled_text();
}

function LockedCard({
  deriving,
  error,
  onSubmit,
  pending,
}: {
  deriving: boolean;
  error: boolean;
  onSubmit: (password: string) => void;
  pending: boolean;
}) {
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
            error={error && !deriving}
            onSubmit={onSubmit}
            pending={pending || deriving}
            pendingLabel={deriving ? m.share_deriving_key() : undefined}
          />
        </CardContent>
      </Card>
    </div>
  );
}

function PlainShareCard({
  code,
  data,
  password,
}: {
  code: string;
  data: PublicShare;
  password?: string;
}) {
  const title = data.title || data.files[0]?.filename || fallbackTitle(data);
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
          <CardTitle
            className="w-full min-w-0 truncate font-medium text-xl tracking-tight"
            title={title}
          >
            {title}
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
