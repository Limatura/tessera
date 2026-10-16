import { IconAlertTriangle, IconCheck, IconPoint } from "@tabler/icons-react";
import { cn } from "@tessera/ui/lib/utils";

import { PASSWORD_MAX_CEILING, passwordRules } from "@/lib/api/password-policy";
import type { PasswordPolicy } from "@/lib/api/password-policy";
import * as m from "@/paraglide/messages";

/**
 * The instance's password rules, ticked off as they are met.
 *
 * Replaces a field error instead of sitting next to one: React Hook Form
 * surfaces one message per field, naming only one unmet rule at a time, but
 * someone missing both a digit and a symbol should learn that in one pass.
 * Rules show from the start; `invalid` only tints the list red.
 *
 * Two refusals are not rules to tick, over-ceiling length and a breach hit
 * (`breached`, from `usePasswordBreach`), and show as soon as they are true,
 * without waiting for `invalid`.
 */
export function PasswordRequirements({
  breached = false,
  className,
  invalid = false,
  policy,
  value,
}: {
  breached?: boolean;
  className?: string;
  invalid?: boolean;
  policy: PasswordPolicy;
  value: string;
}) {
  const rules = passwordRules(policy, value);
  // Server counts characters, not UTF-16 code units.
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
      {breached && <Refusal>{m.auth_password_breached()}</Refusal>}
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
