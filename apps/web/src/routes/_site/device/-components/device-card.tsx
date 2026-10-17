import { useNavigate } from "@tanstack/react-router";
import { Button } from "@tessera/ui/components/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import {
  InputOTP,
  InputOTPGroup,
  InputOTPSeparator,
  InputOTPSlot,
} from "@tessera/ui/components/input-otp";
import { REGEXP_ONLY_CHARS } from "input-otp";
import {
  CircleCheck,
  CircleX,
  ShieldAlert,
  SquareTerminal,
} from "lucide-react";
import { useState } from "react";

import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import {
  useAnswerDeviceAuthorization,
  useDeviceAuthorization,
} from "@/hooks/use-device-authorization";
import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

/** Letters in a code, without its dash; `USER_CODE_LEN` on the server. */
const CODE_LENGTH = 8;
const HALF = CODE_LENGTH / 2;

/** `wdjb-mjht` → `WDJBMJHT`, the way the server reads it too. */
function normalize(code: string): string {
  return code.replaceAll(/[^a-z]/giu, "").toUpperCase();
}

const CARD_CLASS =
  "w-full max-w-sm border-border/60 bg-card/60 shadow-2xl shadow-black/5 backdrop-blur-2xl";

export function DeviceCard({ code }: { code: string | undefined }) {
  const normalized = code === undefined ? "" : normalize(code);
  if (normalized.length !== CODE_LENGTH) {
    return <EnterCode />;
  }
  // Keyed on the code so entering another one starts from a clean slate.
  return <Confirm code={normalized} key={normalized} />;
}

function Heading({
  description,
  icon,
  title,
}: {
  description: string;
  icon: React.ReactNode;
  title: string;
}) {
  return (
    <CardHeader className="items-center text-center">
      <div className="mx-auto mb-2 flex size-10 items-center justify-center rounded-full bg-muted text-muted-foreground">
        {icon}
      </div>
      <CardTitle className="font-medium text-xl tracking-tight">
        {title}
      </CardTitle>
      <CardDescription aria-live="polite">{description}</CardDescription>
    </CardHeader>
  );
}

function EnterCode() {
  const navigate = useNavigate();
  const [value, setValue] = useState("");

  const handleChange = (next: string) => {
    const upper = normalize(next).slice(0, CODE_LENGTH);
    setValue(upper);
    if (upper.length === CODE_LENGTH) {
      navigate({
        search: { code: `${upper.slice(0, HALF)}-${upper.slice(HALF)}` },
        to: "/device",
      });
    }
  };

  return (
    <Card className={CARD_CLASS}>
      <Heading
        description={m.device_enter_description()}
        icon={<SquareTerminal className="size-5" />}
        title={m.device_title()}
      />
      <CardContent>
        <InputOTP
          aria-label={m.device_code_label()}
          autoCapitalize="characters"
          autoComplete="off"
          autoCorrect="off"
          autoFocus
          containerClassName="justify-center"
          lang="en"
          maxLength={CODE_LENGTH}
          onChange={handleChange}
          pattern={REGEXP_ONLY_CHARS}
          value={value}
        >
          <InputOTPGroup>
            {Array.from({ length: HALF }, (_, index) => (
              <InputOTPSlot index={index} key={index} />
            ))}
          </InputOTPGroup>
          <InputOTPSeparator />
          <InputOTPGroup>
            {Array.from({ length: HALF }, (_, index) => (
              <InputOTPSlot index={HALF + index} key={HALF + index} />
            ))}
          </InputOTPGroup>
        </InputOTP>
      </CardContent>
    </Card>
  );
}

function Confirm({ code }: { code: string }) {
  const navigate = useNavigate();
  const formatDate = useDateTimeFormatter();
  const request = useDeviceAuthorization(code);
  const answer = useAnswerDeviceAuthorization();

  const enterAnother = () => navigate({ search: {}, to: "/device" });

  if (answer.isSuccess) {
    return answer.data ? (
      <Card className={CARD_CLASS}>
        <Heading
          description={m.device_approved_description()}
          icon={<CircleCheck className="size-5" />}
          title={m.device_approved_title()}
        />
      </Card>
    ) : (
      <Card className={CARD_CLASS}>
        <Heading
          description={m.device_denied_description()}
          icon={<CircleX className="size-5" />}
          title={m.device_denied_title()}
        />
      </Card>
    );
  }

  if (request.isError) {
    return (
      <Card className={CARD_CLASS}>
        <Heading
          description={m.device_not_found()}
          icon={<CircleX className="size-5" />}
          title={m.device_title()}
        />
        <CardFooter>
          <Button className="w-full" onClick={enterAnother} variant="outline">
            {m.device_try_another()}
          </Button>
        </CardFooter>
      </Card>
    );
  }

  const pending = request.data;
  const display = `${code.slice(0, HALF)}-${code.slice(HALF)}`;

  return (
    <Card aria-busy={request.isPending} className={CARD_CLASS}>
      <Heading
        description={m.device_confirm_description()}
        icon={<SquareTerminal className="size-5" />}
        title={m.device_confirm_title()}
      />
      <CardContent className="flex flex-col gap-5">
        <p className="text-center font-mono text-3xl tracking-[0.2em] tabular-nums">
          {display}
        </p>
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-sm">
          <dt className="text-muted-foreground">{m.device_client()}</dt>
          <dd className="min-w-0 truncate">{pending?.client_name ?? "…"}</dd>
          <dt className="text-muted-foreground">{m.device_requested_from()}</dt>
          <dd className="min-w-0 truncate font-mono tabular-nums">
            {pending?.requested_from ?? "…"}
          </dd>
          <dt className="text-muted-foreground">{m.device_requested_at()}</dt>
          <dd className="tabular-nums">
            {pending ? formatDate(pending.created_at) : "…"}
          </dd>
        </dl>
        <p className="text-muted-foreground text-sm">{m.device_access()}</p>
        <div className="flex gap-2.5 rounded-lg border border-border/60 bg-muted/50 p-3 text-sm">
          <ShieldAlert
            aria-hidden="true"
            className="mt-0.5 size-4 shrink-0 text-muted-foreground"
          />
          <p>{m.device_warning()}</p>
        </div>
        {answer.isError ? (
          <p className="text-destructive text-sm" role="alert">
            {apiErrorMessage(answer.error, m.common_try_again())}
          </p>
        ) : null}
      </CardContent>
      <CardFooter className="flex gap-2">
        <Button
          className="flex-1"
          disabled={!pending || answer.isPending}
          onClick={() => answer.mutate({ approve: false, code })}
          variant="outline"
        >
          {m.device_deny()}
        </Button>
        <Button
          className="flex-1"
          disabled={!pending || answer.isPending}
          onClick={() => answer.mutate({ approve: true, code })}
        >
          {m.device_approve()}
        </Button>
      </CardFooter>
    </Card>
  );
}
