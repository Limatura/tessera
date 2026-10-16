import {
  IconRefresh,
  IconShieldCheck,
  IconShieldOff,
} from "@tabler/icons-react";
import type { components } from "@tessera/api/schema";
import { Badge } from "@tessera/ui/components/badge";
import { Button } from "@tessera/ui/components/button";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import {
  ResponsiveDialog,
  ResponsiveDialogContent,
  ResponsiveDialogDescription,
  ResponsiveDialogFooter,
  ResponsiveDialogHeader,
  ResponsiveDialogTitle,
} from "@tessera/ui/components/responsive-dialog";
import { Skeleton } from "@tessera/ui/components/skeleton";
import { toast } from "@tessera/ui/components/toast";
import { useState } from "react";

import {
  useDisableTotp,
  useMfaStatus,
  useRegenerateRecoveryCodes,
} from "@/hooks/use-mfa";
import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

import { RecoveryCodes } from "./recovery-codes";
import { Section } from "./section";
import { TotpSetupDialog } from "./totp-setup-dialog";

type ErrorBody = components["schemas"]["ErrorBody"];

/** Few enough left that losing the phone could lock the owner out. */
const LOW_RECOVERY_CODES = 3;

type OwnerAction = "disable" | "regenerate";

/**
 * Authenticator-app sign-in. It only guards the password: provider sign-ins
 * and passkeys never ask for a code, which the description says.
 */
export function MfaSection() {
  const status = useMfaStatus();
  const [setupOpen, setSetupOpen] = useState(false);
  const [action, setAction] = useState<OwnerAction | null>(null);

  const renderBody = () => {
    if (status.isPending) {
      return <Skeleton className="h-16 w-full" />;
    }
    if (status.isError) {
      return <p className="text-destructive text-sm">{m.user_mfa_error()}</p>;
    }
    const { totp_enabled: enabled, recovery_codes_left: left } = status.data;
    return (
      <div className="flex flex-wrap items-start gap-x-3 gap-y-3 rounded-lg border p-3">
        <span className="mt-0.5 shrink-0 text-muted-foreground">
          {enabled ? (
            <IconShieldCheck size={18} />
          ) : (
            <IconShieldOff size={18} />
          )}
        </span>
        <div className="min-w-0 flex-1 basis-48 space-y-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="font-medium text-sm">{m.user_mfa_app()}</span>
            <Badge variant={enabled ? "default" : "outline"}>
              {enabled ? m.user_mfa_on() : m.user_mfa_off()}
            </Badge>
          </div>
          {enabled ? (
            <p
              className={
                left <= LOW_RECOVERY_CODES
                  ? "text-amber-600 text-xs dark:text-amber-400"
                  : "text-muted-foreground text-xs"
              }
            >
              {m.user_mfa_codes_left({ count: left })}
            </p>
          ) : null}
        </div>
        {enabled ? (
          <div className="flex w-full gap-2 sm:w-auto">
            <Button
              className="flex-1 sm:flex-none"
              onClick={() => setAction("regenerate")}
              size="sm"
              type="button"
              variant="outline"
            >
              <IconRefresh size={16} />
              {m.user_mfa_regenerate()}
            </Button>
            <Button
              className="flex-1 sm:flex-none"
              onClick={() => setAction("disable")}
              size="sm"
              type="button"
              variant="outline"
            >
              {m.user_mfa_disable()}
            </Button>
          </div>
        ) : (
          <Button
            className="w-full sm:w-auto"
            onClick={() => setSetupOpen(true)}
            size="sm"
            type="button"
            variant="outline"
          >
            {m.user_mfa_setup()}
          </Button>
        )}
      </div>
    );
  };

  return (
    <Section description={m.user_mfa_description()} title={m.user_mfa_title()}>
      {renderBody()}
      <TotpSetupDialog onOpenChange={setSetupOpen} open={setupOpen} />
      <ResponsiveDialog
        onOpenChange={(open) => (open ? null : setAction(null))}
        open={action !== null}
      >
        <ResponsiveDialogContent>
          {action ? (
            <OwnerConfirmForm
              action={action}
              key={action}
              onDone={() => setAction(null)}
            />
          ) : null}
        </ResponsiveDialogContent>
      </ResponsiveDialog>
    </Section>
  );
}

/**
 * Turning it off or replacing the recovery codes asks for a code from the app
 * (or a recovery code). The server also takes the password, but the code works
 * for every account, with or without one, and proves the thing being changed.
 */
function OwnerConfirmForm({
  action,
  onDone,
}: {
  action: OwnerAction;
  onDone: () => void;
}) {
  const disable = useDisableTotp();
  const regenerate = useRegenerateRecoveryCodes();
  const mutation = action === "disable" ? disable : regenerate;
  const [code, setCode] = useState("");
  const [codes, setCodes] = useState<string[] | null>(null);
  const error = mutation.error as Partial<ErrorBody> | null;

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    const body = { code: code.trim() };
    if (action === "disable") {
      disable.mutate(body, {
        onSuccess: () => {
          toast.add({ title: m.user_mfa_disabled_done(), type: "success" });
          onDone();
        },
      });
      return;
    }
    regenerate.mutate(body, { onSuccess: setCodes });
  };

  if (codes) {
    return (
      <>
        <ResponsiveDialogHeader>
          <ResponsiveDialogTitle>
            {m.user_mfa_recovery_title()}
          </ResponsiveDialogTitle>
          <ResponsiveDialogDescription>
            {m.user_mfa_regenerated_done()}
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

  return (
    <form className="contents" onSubmit={handleSubmit}>
      <ResponsiveDialogHeader>
        <ResponsiveDialogTitle>
          {action === "disable"
            ? m.user_mfa_disable_title()
            : m.user_mfa_regenerate_title()}
        </ResponsiveDialogTitle>
        <ResponsiveDialogDescription>
          {action === "disable"
            ? m.user_mfa_disable_description()
            : m.user_mfa_regenerate_description()}
        </ResponsiveDialogDescription>
      </ResponsiveDialogHeader>
      <Field data-invalid={Boolean(error)}>
        <FieldLabel htmlFor="mfa-owner-code">
          {m.user_mfa_code_or_recovery()}
        </FieldLabel>
        <Input
          autoComplete="one-time-code"
          // oxlint-disable-next-line jsx-a11y/no-autofocus -- first input in the dialog
          autoFocus
          id="mfa-owner-code"
          onChange={(event) => setCode(event.target.value)}
          spellCheck={false}
          value={code}
        />
        <FieldDescription>
          {m.user_mfa_code_or_recovery_hint()}
        </FieldDescription>
        {error ? (
          <FieldError>
            {apiErrorMessage(error, m.common_try_again())}
          </FieldError>
        ) : null}
      </Field>
      <ResponsiveDialogFooter>
        <Button onClick={onDone} type="button" variant="outline">
          {m.common_cancel()}
        </Button>
        <Button
          disabled={mutation.isPending || code.trim() === ""}
          type="submit"
          variant={action === "disable" ? "destructive" : "default"}
        >
          {action === "disable"
            ? m.user_mfa_disable()
            : m.user_mfa_regenerate()}
        </Button>
      </ResponsiveDialogFooter>
    </form>
  );
}
