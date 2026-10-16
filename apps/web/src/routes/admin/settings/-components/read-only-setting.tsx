import { Badge } from "@tessera/ui/components/badge";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
} from "@tessera/ui/components/field";
import type * as React from "react";

/**
 * A setting the panel cannot change.
 *
 * A key that can be read but not written cannot stay in the form:
 * `PUT /admin/settings` validates the changes as one batch, and one rejected key
 * sends back the valid fields with it. Dropping the row entirely would suggest
 * the setting does not exist, so show the current value and why it is fixed.
 */
export function ReadOnlySetting({
  label,
  value,
  reason,
  description,
}: {
  label: string;
  /** The value in effect on the server. SECRET keys cannot be read; pass `null` to omit the row. */
  value: string | null;
  /** A short reason shown next to the title, such as "requires restart". */
  reason: string;
  description: React.ReactNode;
}) {
  return (
    <Field orientation="horizontal">
      <FieldContent>
        <FieldLabel>
          {label}
          <Badge variant="outline">{reason}</Badge>
        </FieldLabel>
        <FieldDescription>{description}</FieldDescription>
      </FieldContent>
      {value !== null && (
        <output className="font-mono text-muted-foreground text-sm tabular-nums">
          {value}
        </output>
      )}
    </Field>
  );
}
