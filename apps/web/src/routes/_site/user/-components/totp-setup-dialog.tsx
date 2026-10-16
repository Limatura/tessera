import type { components } from "@tessera/api/schema";
import { Button } from "@tessera/ui/components/button";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import {
  InputOTP,
  InputOTPGroup,
  InputOTPSlot,
} from "@tessera/ui/components/input-otp";
import {
  ResponsiveDialog,
  ResponsiveDialogContent,
  ResponsiveDialogDescription,
  ResponsiveDialogFooter,
  ResponsiveDialogHeader,
  ResponsiveDialogTitle,
} from "@tessera/ui/components/responsive-dialog";
import { REGEXP_ONLY_DIGITS } from "input-otp";
import { useState } from "react";

import { AnimatedQRCode } from "@/components/animated-qr-code";
import { SignInAgain } from "@/components/sign-in-again";
import { useHasPassword } from "@/hooks/use-account";
import { useBeginTotp, useConfirmTotp } from "@/hooks/use-mfa";
import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

import { RecoveryCodes } from "./recovery-codes";

type ErrorBody = components["schemas"]["ErrorBody"];
type TotpSetup = components["schemas"]["TotpSetupResponse"];

const TOTP_LENGTH = 6;

export function TotpSetupDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <ResponsiveDialog onOpenChange={onOpenChange} open={open}>
      <ResponsiveDialogContent>
        {/* Unmounted on close: a half-done setup starts over, never with a stale secret. */}
        {open ? <TotpSetupFlow onDone={() => onOpenChange(false)} /> : null}
      </ResponsiveDialogContent>
    </ResponsiveDialog>
  );
}

/**
 * Three steps: confirm it is the owner, scan and prove the app has the secret,
 * then the recovery codes. Nothing is on until the second step succeeds.
 */
function TotpSetupFlow({ onDone }: { onDone: () => void }) {
  const [setup, setSetup] = useState<TotpSetup | null>(null);
  const [codes, setCodes] = useState<string[] | null>(null);

  if (codes) {
    return (
      <>
        <ResponsiveDialogHeader>
          <ResponsiveDialogTitle>
            {m.user_mfa_recovery_title()}
          </ResponsiveDialogTitle>
          <ResponsiveDialogDescription>
            {m.user_mfa_enabled_done()}
          </ResponsiveDialogDescription>
        </ResponsiveDialogHeader>
        <RecoveryCodes codes={codes} />
        <ResponsiveDialogFooter>
          <Button onClick={onDone} type="button">
            {m.user_mfa_recovery_saved()}
          </Button>
        </ResponsiveDialogFooter>
      </>
    );
  }
  if (setup) {
    return <ScanStep onConfirmed={setCodes} setup={setup} />;
  }
  return <OwnerStep onCancel={onDone} onStarted={setSetup} />;
}

function OwnerStep({
  onStarted,
  onCancel,
}: {
  onStarted: (setup: TotpSetup) => void;
  onCancel: () => void;
}) {
  const hasPassword = useHasPassword();
  const begin = useBeginTotp();
  const [password, setPassword] = useState("");
  const error = begin.error as Partial<ErrorBody> | null;

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    begin.mutate(hasPassword ? password : undefined, { onSuccess: onStarted });
  };

  return (
    <form className="contents" onSubmit={handleSubmit}>
      <ResponsiveDialogHeader>
        <ResponsiveDialogTitle>
          {m.user_mfa_setup_title()}
        </ResponsiveDialogTitle>
        <ResponsiveDialogDescription>
          {m.user_mfa_setup_description()}
        </ResponsiveDialogDescription>
      </ResponsiveDialogHeader>
      {hasPassword ? (
        <Field data-invalid={Boolean(error)}>
          <FieldLabel htmlFor="totp-password">
            {m.user_password_current()}
          </FieldLabel>
          <Input
            autoComplete="current-password"
            // oxlint-disable-next-line jsx-a11y/no-autofocus -- first input in the dialog
            autoFocus
            id="totp-password"
            onChange={(event) => setPassword(event.target.value)}
            type="password"
            value={password}
          />
          {error && error.code !== "reauthentication_required" ? (
            <FieldError>
              {apiErrorMessage(error, m.common_try_again())}
            </FieldError>
          ) : null}
        </Field>
      ) : null}
      {error?.code === "reauthentication_required" ? (
        <SignInAgain message={m.user_reauth_required()} />
      ) : null}
      {!hasPassword && error && error.code !== "reauthentication_required" ? (
        <p className="text-destructive text-sm" role="alert">
          {apiErrorMessage(error, m.common_try_again())}
        </p>
      ) : null}
      <ResponsiveDialogFooter>
        <Button onClick={onCancel} type="button" variant="outline">
          {m.common_cancel()}
        </Button>
        <Button
          disabled={begin.isPending || (hasPassword && password.length === 0)}
          type="submit"
        >
          {m.user_mfa_setup_continue()}
        </Button>
      </ResponsiveDialogFooter>
    </form>
  );
}

function ScanStep({
  setup,
  onConfirmed,
}: {
  setup: TotpSetup;
  onConfirmed: (codes: string[]) => void;
}) {
  const confirm = useConfirmTotp();
  const [code, setCode] = useState("");
  const error = confirm.error as Partial<ErrorBody> | null;

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    confirm.mutate(code, { onSuccess: onConfirmed });
  };

  return (
    <form className="contents" onSubmit={handleSubmit}>
      <ResponsiveDialogHeader>
        <ResponsiveDialogTitle>{m.user_mfa_scan_title()}</ResponsiveDialogTitle>
        <ResponsiveDialogDescription>
          {m.user_mfa_scan_description()}
        </ResponsiveDialogDescription>
      </ResponsiveDialogHeader>
      <div className="flex flex-col items-center gap-3">
        {/* White behind the code even in dark mode: some scanners cannot read inverted QR codes. */}
        <div className="overflow-hidden rounded-lg bg-white">
          <AnimatedQRCode animated={false} size={176} value={setup.uri} />
        </div>
        <p className="text-center text-muted-foreground text-xs">
          {m.user_mfa_scan_manual()}
        </p>
        <code className="select-all break-all rounded bg-muted px-2 py-1 font-mono text-sm">
          {setup.secret}
        </code>
      </div>
      <Field data-invalid={Boolean(error)}>
        <FieldLabel htmlFor="totp-code">{m.user_mfa_code_label()}</FieldLabel>
        <InputOTP
          autoComplete="one-time-code"
          id="totp-code"
          maxLength={TOTP_LENGTH}
          onChange={setCode}
          pattern={REGEXP_ONLY_DIGITS}
          value={code}
        >
          <InputOTPGroup className="mx-auto">
            {Array.from({ length: TOTP_LENGTH }, (_, index) => (
              <InputOTPSlot index={index} key={index} />
            ))}
          </InputOTPGroup>
        </InputOTP>
        <FieldDescription>{m.user_mfa_code_hint()}</FieldDescription>
        {error ? (
          <FieldError>
            {apiErrorMessage(error, m.common_try_again())}
          </FieldError>
        ) : null}
      </Field>
      <ResponsiveDialogFooter>
        <Button
          disabled={confirm.isPending || code.length !== TOTP_LENGTH}
          type="submit"
        >
          {m.user_mfa_enable()}
        </Button>
      </ResponsiveDialogFooter>
    </form>
  );
}
