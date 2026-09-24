import { IconAlertTriangle, IconCheck, IconPoint } from "@tabler/icons-react";
import { cn } from "@tessera/ui/lib/utils";

import { PASSWORD_MAX_CEILING, passwordRules } from "@/lib/api/password-policy";
import type { PasswordPolicy } from "@/lib/api/password-policy";
import * as m from "@/paraglide/messages";

/**
 * The instance's password rules, ticked off as they are met.
 *
 * This replaces a field error rather than sitting next to one. React Hook Form
 * surfaces a single message per field, which would name one unmet rule at a
 * time; someone missing both a digit and a symbol should learn that once, not
 * over two attempts. `invalid` only tints the list — the rules themselves are
 * shown from the start, since telling someone the rules up front beats
 * rejecting them after they have chosen.
 *
 * A password over the ceiling is not a rule to tick, so it appears only when
 * it applies, as it is typed. It does not wait for `invalid`: it is already
 * certain the moment it shows, and the field error it would otherwise be is
 * one this list replaces.
 */
export function PasswordRequirements({
  className,
  invalid = false,
  policy,
  value,
}: {
  className?: string;
  invalid?: boolean;
  policy: PasswordPolicy;
  value: string;
}) {
  const rules = passwordRules(policy, value);
  // The server counts characters, not UTF-16 code units.
  const tooLong = [...value].length > PASSWORD_MAX_CEILING;

  return (
    <ul className={cn("space-y-1 text-muted-foreground text-xs", className)}>
      {rules.map((rule) => (
        <li className="flex items-center gap-1.5" key={rule.id}>
          {rule.satisfied ? (
            <IconCheck aria-hidden className="size-3.5 text-primary" />
          ) : (
            <IconPoint
              aria-hidden
              className={cn("size-3.5", invalid && "text-destructive")}
            />
          )}
          <span
            className={cn(
              rule.satisfied && "text-foreground",
              !rule.satisfied && invalid && "text-destructive"
            )}
          >
            {rule.label}
          </span>
        </li>
      ))}
      {tooLong && <Refusal>{m.auth_password_too_long()}</Refusal>}
    </ul>
  );
}

function Refusal({ children }: { children: string }) {
  return (
    <li className="flex items-center gap-1.5 text-destructive" role="alert">
      <IconAlertTriangle aria-hidden className="size-3.5 shrink-0" />
      <span>{children}</span>
    </li>
  );
}
