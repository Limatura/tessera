import type { components } from "@tessera/api/schema";
import { formatBytes } from "@tessera/ui/lib/format";

import * as m from "@/paraglide/messages";

export type Consequence = components["schemas"]["Consequence"];

/**
 * One sentence for what a settings change would delete on the next sweep,
 * translated by the server's fixed `kind` (its `subject` is English, for the
 * CLI). The switch is exhaustive, so a kind the server adds later fails type
 * checking here instead of showing up in English.
 */
export function consequenceText(consequence: Consequence): string {
  const { count, bytes } = consequence;
  switch (consequence.kind) {
    case "shares": {
      const size =
        bytes === null || bytes === undefined ? "" : ` (${formatBytes(bytes)})`;
      return m.settings_consequence_shares({ count }) + size;
    }
    case "access_log_entries": {
      return m.settings_consequence_access({ count });
    }
    case "login_history": {
      return m.settings_consequence_signins({ count });
    }
    default: {
      const unknown: never = consequence.kind;
      return `${count} ${String(unknown)}`;
    }
  }
}
