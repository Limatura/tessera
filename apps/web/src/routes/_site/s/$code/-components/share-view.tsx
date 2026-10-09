import { useNavigate } from "@tanstack/react-router";
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
  TimerOff,
} from "lucide-react";
import { useState } from "react";

import { useShare } from "@/hooks/use-share";
import * as m from "@/paraglide/messages";

import { LinkShare } from "./link-share";
import { PasswordForm } from "./password-form";
import { RevealedShare } from "./revealed-share";
import { ShareMeta } from "./share-meta";

interface ApiError {
  status?: number;
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

function isApiError(error: unknown): error is ApiError {
  return typeof error === "object" && error !== null && "status" in error;
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
