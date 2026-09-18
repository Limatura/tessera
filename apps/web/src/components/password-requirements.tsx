import { IconCheck, IconPoint } from "@tabler/icons-react";
import { cn } from "@tessera/ui/lib/utils";

import { passwordRules } from "@/lib/api/password-policy";
import type { PasswordPolicy } from "@/lib/api/password-policy";

/**
 * The instance's password rules, ticked off as they are met.
 *
 * This replaces a field error rather than sitting next to one. React Hook Form
 * surfaces a single message per field, which would name one unmet rule at a
 * time; someone missing both a digit and a symbol should learn that once, not
 * over two attempts. `invalid` only tints the list — the rules themselves are
 * shown from the start, since telling someone the rules up front beats
 * rejecting them after they have chosen.
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
    </ul>
  );
}
